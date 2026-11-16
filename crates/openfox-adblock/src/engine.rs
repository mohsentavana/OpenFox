use crate::rules::{ListKind, NetworkRule, ResourceType};
use crate::cosmetic::CosmeticEngine;
use std::collections::HashMap;

/// What the engine decided for one request, and why. The rule text travels
/// with the decision so devtools can show exactly which filter fired —
/// a blocked request without attribution is a bug report waiting to happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Blocked { kind: ListKind, rule: String },
    Allowed { exception: Option<String> },
    Default,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Totals {
    pub ads: u64,
    pub trackers: u64,
    pub custom: u64,
}

impl Totals {
    pub fn record(&mut self, kind: ListKind) {
        match kind {
            ListKind::Ads => self.ads += 1,
            ListKind::Trackers => self.trackers += 1,
            ListKind::Custom => self.custom += 1,
        }
    }

    pub fn requests(&self) -> u64 {
        self.ads + self.trackers + self.custom
    }
}

/// The matcher consulted before any socket opens. Rules with `||` hosts are
/// indexed by that host; everything else is a global scan. Exceptions are
/// checked first, always — an allowlist entry must beat a blocklist entry.
pub struct Engine {
    rules: Vec<NetworkRule>,
    blocks_host: HashMap<String, Vec<usize>>,
    blocks_global: Vec<usize>,
    exceptions_host: HashMap<String, Vec<usize>>,
    exceptions_global: Vec<usize>,
    cosmetics: CosmeticEngine,
    totals: Totals,
    hosts: HashMap<String, u64>,
    ignored: usize,
}

impl Engine {
    pub fn new() -> Self {
        Engine {
            rules: Vec::new(),
            blocks_host: HashMap::new(),
            blocks_global: Vec::new(),
            exceptions_host: HashMap::new(),
            exceptions_global: Vec::new(),
            cosmetics: CosmeticEngine::new(),
            totals: Totals::default(),
            hosts: HashMap::new(),
            ignored: 0,
        }
    }

    /// Loads one list text. Cosmetic rules land in the cosmetic engine;
    /// unparseable or option-unsupported lines are counted, not guessed at.
    pub fn add_list(&mut self, text: &str, kind: ListKind) {
        for line in text.lines() {
            if let Some((hosts, selector)) = split_cosmetic(line) {
                self.cosmetics.add(hosts, selector);
                continue;
            }
            match NetworkRule::parse(line, kind) {
                Ok(Some(rule)) => self.insert(rule),
                Ok(None) => {}
                Err(_) => self.ignored += 1,
            }
        }
    }

    fn insert(&mut self, rule: NetworkRule) {
        let idx = self.rules.len();
        match (&rule.host_anchor, rule.exception) {
            (Some(host), false) => self.blocks_host.entry(host.clone()).or_default().push(idx),
            (Some(host), true) => self.exceptions_host.entry(host.clone()).or_default().push(idx),
            (None, true) => self.exceptions_global.push(idx),
            (None, false) => self.blocks_global.push(idx),
        }
        self.rules.push(rule);
    }

    /// The pre-socket gate. `source_host` is the top frame's host, needed by
    /// `third-party` rules and by the first-party exception rule.
    pub fn check(&mut self, url: &url::Url, resource: ResourceType, source_host: Option<&str>) -> Decision {
        if let Some(rule) = self.find_match(&self.exceptions_host, &self.exceptions_global, url, resource, source_host) {
            return Decision::Allowed { exception: Some(rule.source.clone()) };
        }
        if let Some(rule) = self.find_match(&self.blocks_host, &self.blocks_global, url, resource, source_host) {
            // Copy out of the borrow before the counters take &mut self.
            let (kind, host_anchor, source) = (rule.kind, rule.host_anchor.clone(), rule.source.clone());
            self.totals.record(kind);
            let host = host_anchor
                .or_else(|| url.host_str().map(str::to_owned))
                .unwrap_or_else(|| "(global rule)".into());
            *self.hosts.entry(host).or_insert(0) += 1;
            return Decision::Blocked { kind, rule: source };
        }
        Decision::Default
    }

    fn find_match<'a>(
        &'a self,
        host_map: &HashMap<String, Vec<usize>>,
        globals: &[usize],
        url: &url::Url,
        resource: ResourceType,
        source_host: Option<&str>,
    ) -> Option<&'a NetworkRule> {
        for idx in self.candidates(host_map, url) {
            if self.rules[idx].matches(url, resource, source_host) {
                return Some(&self.rules[idx]);
            }
        }
        for &idx in globals {
            if self.rules[idx].matches(url, resource, source_host) {
                return Some(&self.rules[idx]);
            }
        }
        None
    }

    /// Rule indices for every label suffix of the URL's host: a rule anchored
    /// at `doubleclick.net` must see requests to `ad.doubleclick.net`.
    fn candidates(&self, host_map: &HashMap<String, Vec<usize>>, url: &url::Url) -> Vec<usize> {
        let mut out = Vec::new();
        let Some(host) = url.host_str() else { return out };
        let mut labels: Vec<&str> = host.split('.').collect();
        while labels.len() > 1 {
            let suffix = labels.join(".");
            if let Some(indices) = host_map.get(&suffix.to_ascii_lowercase()) {
                out.extend(indices.iter().copied());
            }
            labels.remove(0);
        }
        out
    }

    pub fn cosmetics(&self) -> &CosmeticEngine {
        &self.cosmetics
    }

    pub fn totals(&self) -> Totals {
        self.totals
    }

    /// The hosts behind the shield popover, most-blocked first.
    pub fn top_hosts(&self, limit: usize) -> Vec<(String, u64)> {
        let mut hosts: Vec<(String, u64)> = self.hosts.iter().map(|(h, c)| (h.clone(), *c)).collect();
        hosts.sort_by(|a, b| b.1.cmp(&a.1));
        hosts.truncate(limit);
        hosts
    }

    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    pub fn ignored_count(&self) -> usize {
        self.ignored
    }
}

fn split_cosmetic(line: &str) -> Option<(Vec<String>, String)> {
    let marker = if line.contains("#@#") {
        return None; // exception cosmetics: phase 2, never silently ignored
    } else {
        "##"
    };
    let (hosts, selector) = line.split_once(marker)?;
    if selector.trim().is_empty() {
        return None;
    }
    let hosts: Vec<String> = hosts
        .split(',')
        .map(|h| h.trim().to_ascii_lowercase())
        .filter(|h| !h.is_empty())
        .collect();
    Some((hosts, selector.trim().to_owned()))
}
