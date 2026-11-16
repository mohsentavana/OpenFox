//! The command line. Every flag lands somewhere real — no decorative flags.

use clap::Parser;
use std::path::PathBuf;

/// OpenFox's command line. `--headless` boots the browser without a
/// window, `--mcp-stdio` serves the agent transport on stdin/stdout,
/// `--private` swaps the profile for an ephemeral one, `--profile`
/// overrides the profile directory, `--url` opens one tab at start, and
/// `--version-print` prints the version and exits. Anything a user cannot
/// point at a behavior does not get a flag.
#[derive(Parser, Debug)]
#[command(name = "openfox", about = "The super browser. Pure Rust, no ads, MCP-native.")]
pub struct Cli {
    /// Run without a window: agents and CI render to the CPU PNG path.
    #[arg(long)]
    pub headless: bool,

    /// Serve MCP over stdio (implies headless).
    #[arg(long)]
    pub mcp_stdio: bool,

    /// Serve MCP over localhost HTTP on this port (implies headless).
    #[arg(long, value_name = "PORT")]
    pub mcp_http: Option<u16>,

    /// Private session: ephemeral profile under the temp root, nothing
    /// written to the user's real profile.
    #[arg(long)]
    pub private: bool,

    /// Profile directory (defaults per-platform).
    #[arg(long, value_name = "DIR")]
    pub profile: Option<PathBuf>,

    /// URL to open at start (repeatable).
    #[arg(long = "url", value_name = "URL")]
    pub urls: Vec<String>,

    /// URLs to open at start (positional, the Firefox way).
    #[arg(value_name = "URL")]
    pub positional_urls: Vec<String>,

    /// Print the version and exit.
    #[arg(long, default_value_t = false)]
    pub version_print: bool,
}

/// Parsed options. Decoupled from clap so the rest of the shell reads
/// intent, not argv.
#[derive(Debug, Clone)]
pub struct AppOptions {
    pub headless: bool,
    pub mcp_stdio: bool,
    pub mcp_http: Option<u16>,
    pub private: bool,
    pub profile: Option<PathBuf>,
    pub urls: Vec<String>,
    pub version_print: bool,
}

impl From<Cli> for AppOptions {
    fn from(cli: Cli) -> AppOptions {
        AppOptions {
            // An agent transport without a window is the whole point; a
            // window without a transport is just a browser.
            headless: cli.headless || cli.mcp_stdio || cli.mcp_http.is_some(),
            mcp_stdio: cli.mcp_stdio,
            mcp_http: cli.mcp_http,
            private: cli.private,
            profile: cli.profile,
            urls: cli
                .urls
                .into_iter()
                .chain(cli.positional_urls)
                .collect(),
            version_print: cli.version_print,
        }
    }
}

impl AppOptions {
    /// Resolves this session's profile directory: the flag wins, private
    /// mode gets a throwaway directory under the temp root (nothing lands
    /// in the real profile), otherwise the platform default.
    pub fn profile_dir(&self) -> PathBuf {
        if let Some(dir) = &self.profile {
            return dir.clone();
        }
        if self.private {
            return std::env::temp_dir().join(format!("openfox-private-{}", std::process::id()));
        }
        crate::app::default_profile_root()
    }
}
