use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

const HEAD_CAP: usize = 8 * 1024;
const BODY_CAP: usize = 1024 * 1024;

/// Streamable HTTP on localhost: one POST carries one JSON-RPC message,
/// the response is plain JSON, no SSE. Bound to 127.0.0.1 — the browser is
/// driven locally by its owner's agent, not by whoever can reach the port.
pub async fn serve(backend: Arc<Mutex<dyn crate::Backend>>, port: u16) -> std::io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port)).await?;
    loop {
        let (stream, _) = listener.accept().await?;
        let backend = backend.clone();
        tokio::spawn(handle_conn(stream, backend));
    }
}

async fn handle_conn(mut stream: TcpStream, backend: Arc<Mutex<dyn crate::Backend>>) {
    let mut reader = BufReader::new(&mut stream);
    let mut head = Vec::with_capacity(1024);
    loop {
        let mut byte = [0u8; 512];
        match reader.read(&mut byte).await {
            Ok(0) | Err(_) => return,
            Ok(n) => {
                head.extend_from_slice(&byte[..n]);
                if head.windows(4).any(|w| w == b"\r\n\r\n") || head.len() > HEAD_CAP {
                    break;
                }
            }
        }
    }

    let head_text = String::from_utf8_lossy(&head);
    let content_length = head_text
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.trim().eq_ignore_ascii_case("content-length") {
                value.trim().parse::<usize>().ok()
            } else {
                None
            }
        })
        .unwrap_or(0)
        .min(BODY_CAP);

    let head_len = head.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4).unwrap_or(head.len());
    let mut body = head[head_len.min(head.len())..].to_vec();
    while body.len() < content_length {
        let mut chunk = vec![0u8; (content_length - body.len()).min(64 * 1024)];
        match reader.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => body.extend_from_slice(&chunk[..n]),
        }
    }

    let is_post = head_text.starts_with("POST");
    let is_mcp_path = head_text
        .lines()
        .next()
        .is_some_and(|request| request.contains("/mcp"));

    let payload = if is_post && is_mcp_path && !body.is_empty() {
        // Dispatch on the blocking pool: backend handlers may block on the
        // browser's own runtime, which is illegal on this async thread.
        let text = String::from_utf8_lossy(&body).into_owned();
        tokio::task::spawn_blocking(move || crate::handle_message(&backend, &text))
            .await
            .expect("mcp dispatch task")
            .unwrap_or_else(|| "{}".to_owned())
    } else {
        serde_json::to_string(&crate::jsonrpc::Response::error(
            None,
            crate::jsonrpc::INVALID_REQUEST,
            "POST /mcp with a JSON-RPC body is the only surface",
        ))
        .expect("error response serializes")
    };

    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
        payload.len(),
        payload
    );
    let _ = (&mut stream).write_all(response.as_bytes()).await;
    let _ = (&mut stream).flush().await;
}
