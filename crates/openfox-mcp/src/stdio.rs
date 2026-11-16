use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Line-delimited JSON over stdio: the transport agents spawn with
/// `--headless --mcp-stdio`. One request per line, one response per line,
/// notifications swallowed silently. Dispatch runs on the blocking pool so
/// backend methods may block on the browser's own runtime legally.
pub async fn serve(backend: Arc<Mutex<dyn crate::Backend>>) -> std::io::Result<()> {
    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();
    let mut stdout = tokio::io::stdout();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let backend = backend.clone();
        let response = tokio::task::spawn_blocking(move || crate::handle_message(&backend, &line))
            .await
            .expect("mcp dispatch task");
        if let Some(response) = response {
            stdout.write_all(response.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }
    }
    Ok(())
}
