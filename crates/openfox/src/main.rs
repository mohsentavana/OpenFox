// Release Windows builds launch as GUI apps: no console window spawns when
// the browser is double-clicked. Piped stdio (the MCP transports) keeps
// working — subsystem only governs console *allocation*, not handles.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

mod about;
mod app;
mod cli;
mod headless;
mod window;

use app::App;
use clap::Parser;
use std::sync::{Arc, Mutex};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = cli::AppOptions::from(cli::Cli::parse());
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "openfox=info".into()),
        )
        .compact()
        .init();

    if options.version_print {
        println!("openfox {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let profile_dir = options.profile_dir();
    let mut app = App::new(&profile_dir)?;
    for url in &options.urls {
        let tab = app.open_tab(Some(url.clone()));
        let result = app.navigate_sync(tab, url);
        tracing::info!(target: "openfox::nav", "{}", result);
    }
    let app: Arc<Mutex<dyn openfox_mcp::Backend>> = Arc::new(Mutex::new(app));

    if options.mcp_stdio || options.mcp_http.is_some() {
        let loop_ = headless::HeadlessLoop::new(app.clone());
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        runtime.block_on(async move {
            let mut tasks = Vec::new();
            if let Some(port) = options.mcp_http {
                let server = loop_.clone_for_http();
                tasks.push(tokio::spawn(async move {
                    if let Err(error) = server.serve_http(port).await {
                        eprintln!("mcp http: {error}");
                    }
                }));
            }
            if options.mcp_stdio {
                if let Err(error) = loop_.serve_stdio().await {
                    eprintln!("mcp stdio: {error}");
                }
            }
            for task in tasks {
                let _ = task.await;
            }
            Ok::<(), std::io::Error>(())
        })?;
        // The browser's own runtime cannot drop inside the async context
        // that just served the transports; the process exits here anyway,
        // so the browser is deliberately released to the OS instead.
        std::mem::forget(app);
        return Ok(());
    }

    if options.headless {
        // Headless without a transport is a script that forgot itself.
        eprintln!("headless without a transport: pass --mcp-stdio or --mcp-http PORT");
        return Ok(());
    }

    run_desktop(app);
    Ok(())
}

/// Desktop mode. Startup has completed — URLs opened, session written.
/// The Rune window mounts through rune's own app entry at the pinned `dev`
/// rev (docs/BUILD.md); until that integration lands, this prints the real
/// process state and parks. Nothing here pretends to be a frame loop.
fn run_desktop(app: Arc<Mutex<dyn openfox_mcp::Backend>>) {
    window::run(app);
}
