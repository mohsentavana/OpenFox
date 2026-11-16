use openfox_adblock::{Decision, Engine as AdblockEngine, ListKind, ResourceType};
use openfox_core::{LoadState, Settings, TabId, TabManager};
use openfox_devtools::Devtools;
use openfox_engine::{css, style::selector_matches, Page};
use openfox_ext::{ExtContext, ExtRuntime, Manifest};
use openfox_mcp::{Backend, Direction, ToolError};
use openfox_net::{Client, PrivacyHeaders, Request};
use openfox_render::Compositor;
use openfox_storage::Store;
use base64::Engine as _;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// The browser, assembled. One struct, one runtime: the net stack, the
/// adblock policy engine, the page engine, the compositor, storage,
/// devtools and the extension runtime — the MCP backend implements over
/// exactly what a user sees, nothing more.
pub struct App {
    runtime: Option<tokio::runtime::Runtime>,
    pub core: TabManager,
    net: Client,
    adblock: AdblockEngine,
    devtools: Devtools,
    store: Store,
    compositor: Option<Compositor>,
    pages: HashMap<TabId, Page>,
    typed: HashMap<TabId, String>,
    ext_runtime: ExtRuntime,
    settings: Settings,
    session: openfox_core::Session,
    responses_seen: u64,
    responses_bytes: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("profile {0}: {1}")]
    Profile(String, std::io::Error),
    #[error("storage: {0}")]
    Storage(#[from] openfox_storage::StoreError),
    #[error("session: {0}")]
    Session(#[from] openfox_core::session::SessionError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

impl App {
    pub fn new(profile_dir: &Path) -> Result<App, AppError> {
        std::fs::create_dir_all(profile_dir).map_err(|e| AppError::Profile(profile_dir.display().to_string(), e))?;
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;

        let settings = openfox_core::settings::Profile::open("default", profile_dir)?.settings;
        let store = Store::open(&profile_dir.join("openfox.db"))?;
        let session = openfox_core::Session::new(profile_dir)?;
        let core = session.restore()?;

        let privacy = PrivacyHeaders {
            gpc: settings.privacy_signals,
            dnt: settings.privacy_signals,
            trim_referer: settings.trim_referer,
            user_agent: if settings.user_agent.is_empty() {
                openfox_net::privacy::PLATFORM_UA.to_owned()
            } else {
                settings.user_agent.clone()
            },
        };
        let net = Client::new(privacy, settings.first_party_cookies);

        let mut adblock = AdblockEngine::new();
        adblock.add_list(openfox_adblock::lists::builtin_ads(), ListKind::Ads);
        adblock.add_list(openfox_adblock::lists::builtin_trackers(), ListKind::Trackers);

        // The compositor is optional: no adapter, no screenshots — the rest
        // of the browser is fully functional without a GPU.
        let compositor = openfox_platform::PlatformServices::load().ok().and_then(|services| {
            runtime
                .block_on(async { Compositor::new(services.font_bytes).await })
                .ok()
        });

        Ok(App {
            runtime: Some(runtime),
            core,
            net,
            adblock,
            devtools: Devtools::new(),
            store,
            compositor,
            pages: HashMap::new(),
            typed: HashMap::new(),
            ext_runtime: ExtRuntime::new(),
            settings,
            session,
            responses_seen: 0,
            responses_bytes: 0,
        })
    }

    pub fn open_tab(&mut self, url: Option<String>) -> TabId {
        self.core.open(url)
    }

    /// Normalizes user input the way an address bar does: scheme or host,
    /// otherwise search. Never guesses a scheme other than https.
    fn normalize_input(&self, input: &str) -> String {
        let input = input.trim();
        if input.contains("://") {
            return input.to_owned();
        }
        let looks_like_host = input
            .split(['/', '?', '#'])
            .next()
            .unwrap_or_default()
            .rsplit('.')
            .next()
            .is_some_and(|tld| tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphanumeric()));
        if looks_like_host {
            format!("https://{input}")
        } else {
            self.settings.search_template.replace("{}", input)
        }
    }

    /// Navigate and answer as data: what loaded, what it cost, what was
    /// blocked. This one return value is what the chrome, MCP and devtools
    /// each render their own way.
    pub fn navigate_sync(&mut self, tab: TabId, input: &str) -> Value {
        // The runtime moves out of `self` for the duration: the future
        // borrows `&mut self`, and a borrowed field cannot drive it.
        let runtime = self.runtime.take().expect("app runtime");
        let out = runtime.block_on(async { self.navigate(tab, input).await });
        self.runtime = Some(runtime);
        out
    }

    async fn navigate(&mut self, tab: TabId, input: &str) -> Value {
        let url = self.normalize_input(input);
        let Ok(parsed) = url::Url::parse(&url) else {
            return json!({ "ok": false, "error": format!("not a URL: {url}") });
        };

        // The shield decision happens before any socket opens.
        match self.adblock.check(&parsed, ResourceType::Document, None) {
            Decision::Blocked { rule, .. } => {
                self.devtools.network.record(&url, "GET", 0, 0, 0, true, "shield");
                self.devtools.console.error(format!("document blocked by policy: {rule}"));
                self.core.get_mut(tab).map(|t| {
                    t.state = LoadState::Failed { code: 0 };
                    t.shield.ads_blocked += 1;
                    t.shield.requests_saved += 1;
                });
                return json!({ "ok": true, "blocked": true, "url": url, "rule": rule });
            }
            Decision::Allowed { .. } | Decision::Default => {}
        }

        let referer = self.core.get(tab).and_then(|t| t.url().map(str::to_owned));
        let mut req = Request::get(url.clone());
        req.referer = referer;
        let started = Instant::now();

        match self.net.fetch(req).await {
            Ok(response) => {
                let elapsed_ms = started.elapsed().as_millis() as u64;
                let bytes = response.body.len() as u64;
                self.responses_seen += 1;
                self.responses_bytes += bytes;
                let html = String::from_utf8_lossy(&response.body).into_owned();

                // Subresources go through the same shield: stylesheets are
                // fetched here so the counters count something real.
                let mut blocked_here = 0;
                let mut css_texts: Vec<String> = Vec::new();
                for href in Page::linked_stylesheets(&html) {
                    let Ok(absolute) = parsed.join(&href) else { continue };
                    match self.adblock.check(&absolute, ResourceType::Stylesheet, parsed.host_str()) {
                        Decision::Blocked { rule, .. } => {
                            blocked_here += 1;
                            let host = absolute.host_str().unwrap_or_default().to_owned();
                            self.devtools.network.record(absolute.as_str(), "GET", 0, 0, 0, true, "shield");
                            tracing::info!(target: "openfox::shield", "blocked stylesheet: {rule}");
                        }
                        _ => {
                            let mut sub = Request::get(absolute.as_str());
                            sub.top_frame = url.clone();
                            if let Ok(css_response) = self.net.fetch(sub).await {
                                css_texts.push(String::from_utf8_lossy(&css_response.body).into_owned());
                                self.devtools.network.record(
                                    absolute.as_str(),
                                    "GET",
                                    css_response.status,
                                    css_response.elapsed.as_millis() as u64,
                                    css_response.body.len() as u64,
                                    false,
                                    css_response.via,
                                );
                            }
                        }
                    }
                }

                let page = Page::build(&html, &css_texts.iter().map(String::as_str).collect::<Vec<_>>());
                let title = page.title.clone().unwrap_or_else(|| parsed.host_str().unwrap_or("").to_owned());

                let requests_before = self
                    .core
                    .get(tab)
                    .map(|t| t.shield.requests_saved)
                    .unwrap_or_default();
                let saved_estimate = self.estimate_saved_bytes(requests_before + blocked_here);
                if let Some(t) = self.core.get_mut(tab) {
                    t.push(url.clone(), title.clone());
                    t.state = LoadState::Loaded { status: response.status, bytes };
                    t.shield.requests_saved += blocked_here;
                    t.shield.ads_blocked += blocked_here;
                    t.shield.bytes_saved = saved_estimate;
                    t.loaded_at = Some(Instant::now());
                }
                self.pages.insert(tab, page);
                self.devtools
                    .network
                    .record(&url, "GET", response.status, elapsed_ms, bytes, false, response.via);
                let _ = self.store.record_visit(&url, &title, blocked_here as u32);
                let _ = self.session.save(&self.core);

                let shield = self.core.get(tab).map(|t| t.shield).unwrap_or_default();
                json!({
                    "ok": true,
                    "url": response.final_url,
                    "title": title,
                    "status": response.status,
                    "bytes": bytes,
                    "via": response.via,
                    "elapsedMs": elapsed_ms,
                    "blocked": shield.total_blocked(),
                    "savedBytesEstimate": shield.bytes_saved,
                })
            }
            Err(error) => {
                self.devtools.console.error(format!("{url}: {error}"));
                self.core.get_mut(tab).map(|t| t.state = LoadState::Failed { code: 0 });
                json!({ "ok": false, "url": url, "error": error.to_string() })
            }
        }
    }

    fn estimate_saved_bytes(&self, requests: u32) -> u64 {
        if self.responses_seen == 0 {
            return 0;
        }
        let mean = self.responses_bytes / self.responses_seen;
        requests as u64 * mean
    }

    fn active_page(&self) -> Result<(TabId, &Page), ToolError> {
        let tab = self
            .core
            .active()
            .map(|t| t.id)
            .ok_or_else(|| ToolError::new("no active tab"))?;
        let page = self
            .pages
            .get(&tab)
            .ok_or_else(|| ToolError::new("the active tab has no loaded page"))?;
        Ok((tab, page))
    }

    /// Query the loaded page with the engine's own selector matcher.
    fn query(&self, selector: &str) -> Result<Vec<Value>, ToolError> {
        let (_, page) = self.active_page()?;
        let sheet = css::parse_stylesheet(&format!("{selector} {{}}"));
        let mut out = Vec::new();
        for rule in &sheet.rules {
            for sel in &rule.selectors {
                page.dom.walk(page.dom.root, &mut |id, node| {
                    if node.element().is_some() && selector_matches(&page.dom, id, &sel.parts) {
                        let text = page.dom.inner_text(id);
                        out.push(json!({
                            "tag": page.dom.tag(id),
                            "id": page.dom.attr(id, "id"),
                            "classes": page.dom.classes(id),
                            "text": text.chars().take(80).collect::<String>(),
                        }));
                    }
                });
            }
        }
        Ok(out)
    }

    fn snapshot(&self, depth: u32) -> Result<Value, ToolError> {
        let (_, page) = self.active_page()?;
        let body = page
            .dom
            .find_first_tag("body")
            .unwrap_or(page.dom.root);

        fn node_json(page: &Page, id: openfox_engine::NodeId, depth: u32) -> Value {
            let dom = &page.dom;
            if let Some(text) = dom.node(id).text() {
                return json!(text.chars().take(80).collect::<String>());
            }
            let children: Vec<Value> = if depth > 0 {
                dom.children(id).iter().map(|child| node_json(page, *child, depth - 1)).collect()
            } else {
                Vec::new()
            };
            json!({
                "tag": dom.tag(id),
                "id": dom.attr(id, "id"),
                "children": children,
            })
        }
        Ok(node_json(page, body, depth))
    }
}

impl Backend for App {
    fn navigate(&mut self, url: &str) -> Result<Value, ToolError> {
        let tab = self
            .core
            .active()
            .map(|t| t.id)
            .unwrap_or_else(|| self.core.open(None));
        Ok(self.navigate_sync(tab, url))
    }

    fn go(&mut self, direction: Direction) -> Result<Value, ToolError> {
        let tab = self
            .core
            .active()
            .map(|t| t.id)
            .ok_or_else(|| ToolError::new("no active tab"))?;
        let next = {
            let active = self.core.get_mut(tab).expect("active tab exists");
            match direction {
                Direction::Back => active.back().map(|e| e.url.clone()),
                Direction::Forward => active.forward().map(|e| e.url.clone()),
                Direction::Reload => active.url().map(str::to_owned),
            }
        };
        match next {
            Some(url) => Ok(self.navigate_sync(tab, &url)),
            None => Ok(json!({ "ok": false, "error": "no history in that direction" })),
        }
    }

    fn tabs_list(&mut self) -> Result<Value, ToolError> {
        let tabs: Vec<Value> = self
            .core
            .iter()
            .map(|tab| {
                json!({
                    "id": tab.id.0,
                    "title": tab.current().map(|e| e.title.clone()).unwrap_or_default(),
                    "url": tab.current().map(|e| e.url.clone()).unwrap_or_default(),
                    "state": tab.state.is_loading(),
                    "shield": tab.shield.total_blocked(),
                })
            })
            .collect();
        Ok(json!({ "tabs": tabs }))
    }

    fn tabs_new(&mut self, url: Option<&str>) -> Result<Value, ToolError> {
        let id = self.core.open(url.map(str::to_owned));
        let result = match url {
            Some(url) => self.navigate_sync(id, url),
            None => json!({ "ok": true, "tab": id.0, "note": "about:fox" }),
        };
        Ok(result)
    }

    fn tabs_close(&mut self, tab: u64) -> Result<Value, ToolError> {
        let id = TabId(tab);
        let closed = self.core.close(id);
        self.pages.remove(&id);
        self.typed.remove(&id);
        Ok(json!({ "ok": closed, "tab": tab }))
    }

    fn tabs_select(&mut self, tab: u64) -> Result<Value, ToolError> {
        let selected = self.core.select(TabId(tab));
        Ok(json!({ "ok": selected, "tab": tab }))
    }

    fn dom_query(&mut self, selector: &str) -> Result<Value, ToolError> {
        Ok(json!({ "matches": self.query(selector)? }))
    }

    fn dom_snapshot(&mut self, depth: u32) -> Result<Value, ToolError> {
        self.snapshot(depth)
    }

    fn click(&mut self, selector: &str) -> Result<Value, ToolError> {
        let matches = self.query(selector)?;
        let first = matches
            .first()
            .ok_or_else(|| ToolError::new(format!("no match for {selector}")))?;
        if let Some(tab) = self.core.active() {
            self.typed.entry(tab.id).or_default().clear();
        }
        Ok(json!({ "clicked": first }))
    }

    fn type_text(&mut self, selector: Option<&str>, text: &str) -> Result<Value, ToolError> {
        if let Some(selector) = selector {
            self.click(selector)?;
        }
        let tab = self
            .core
            .active()
            .map(|t| t.id)
            .ok_or_else(|| ToolError::new("no active tab"))?;
        let buffer = self.typed.entry(tab).or_default();
        buffer.push_str(text);
        Ok(json!({ "tab": tab.0, "value": buffer.clone() }))
    }

    fn screenshot(&mut self, full_page: bool) -> Result<Value, ToolError> {
        let (tab, page) = self.active_page()?;
        let (w, h) = if full_page {
            (openfox_engine::VIEWPORT_WIDTH, openfox_engine::VIEWPORT_HEIGHT)
        } else {
            (800.0, 600.0)
        };
        let list = page.display_list();
        let png = self
            .compositor
            .as_mut()
            .ok_or_else(|| ToolError::new("no GPU adapter: screenshots unavailable in this environment"))?
            .render_png(&list, w as u32, h as u32)
            .map_err(|e| ToolError::new(format!("render: {e}")))?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(png);
        Ok(json!({ "tab": tab.0, "format": "png", "base64": encoded }))
    }

    fn network_log(&mut self, filter: Option<&str>) -> Result<Value, ToolError> {
        Ok(self.devtools.network.to_json(filter))
    }

    fn console_log(&mut self, level: Option<u8>) -> Result<Value, ToolError> {
        Ok(self.devtools.console.to_json(level))
    }

    fn adblock_stats(&mut self) -> Result<Value, ToolError> {
        let totals = self.adblock.totals();
        let hosts: Vec<Value> = self
            .adblock
            .top_hosts(6)
            .into_iter()
            .map(|(host, count)| json!({ "host": host, "blocked": count }))
            .collect();
        Ok(json!({
            "adsBlocked": totals.ads,
            "trackersBlocked": totals.trackers,
            "requestsSaved": totals.requests(),
            "savedBytesEstimate": self.estimate_saved_bytes(totals.requests() as u32),
            "rules": self.adblock.rule_count(),
            "ignoredRules": self.adblock.ignored_count(),
            "topHosts": hosts,
        }))
    }

    fn history_search(&mut self, query: &str, limit: u32) -> Result<Value, ToolError> {
        let hits: Vec<Value> = self
            .store
            .search_history(query, limit)
            .map_err(|e| ToolError::new(format!("storage: {e}")))?
            .into_iter()
            .map(|hit| {
                json!({
                    "url": hit.url,
                    "title": hit.title,
                    "visits": hit.visit_count,
                    "score": hit.score,
                })
            })
            .collect();
        Ok(json!({ "hits": hits }))
    }

    fn ext_run(&mut self, wasm_base64: &str) -> Result<Value, ToolError> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(wasm_base64)
            .map_err(|e| ToolError::new(format!("base64: {e}")))?;
        let manifest = Manifest {
            name: "inline".into(),
            version: "0".into(),
            permissions: Vec::new(),
        };
        let tab = self.core.active().map(|t| t.id).map(|t| t.0 as i64).unwrap_or(0);
        let shield = self.adblock.totals().requests() as i64;
        let ext = self
            .ext_runtime
            .compile(&bytes, manifest)
            .map_err(|e| ToolError::new(format!("wasm: {e}")))?;
        let result = self
            .ext_runtime
            .run(&ext, ExtContext { tab, shield_blocked: shield })
            .map_err(|e| ToolError::new(format!("wasm: {e}")))?;
        for log in &result.logs {
            self.devtools.console.info(format!("ext: {log}"));
        }
        Ok(json!({ "report": result.report, "logs": result.logs }))
    }
}

/// Where profiles live when no flag says otherwise.
pub fn default_profile_root() -> PathBuf {
    #[cfg(windows)]
    {
        openfox_platform::windows::profile_root()
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("openfox-data")
    }
}
