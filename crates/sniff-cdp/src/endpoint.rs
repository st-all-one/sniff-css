//! Resolve a human-friendly DevTools endpoint to a `ws://` URL.
//!
//! The CLI and MCP server can be pointed at a browser that is already
//! running (for example Chromium inside the GUI container) using either a
//! bare `ws://...` endpoint or a plain HTTP origin like `http://127.0.0.1:9222`
//! (or `127.0.0.1:9222`). In the latter case we fetch the standard
//! `/json/version` document and read its `webSocketDebuggerUrl` field.

use crate::client::{CdpError, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Turn `endpoint` into a `ws://` DevTools URL.
///
/// Accepts:
/// - `ws://host:port/path` / `wss://...` — passed through unchanged;
/// - `http://host:port` (or `host:port`) — resolved via `GET /json/version`;
/// - `http://host:port/json/version` — the version document is used directly.
///
/// Defaults the port to `9222` when absent. No TLS verification is performed
/// on the `https` scheme; it is passed through for a reverse proxy that
/// terminates TLS in front of the browser.
pub async fn resolve_endpoint(endpoint: &str) -> Result<String> {
    let s = endpoint.trim();
    if s.starts_with("ws://") || s.starts_with("wss://") {
        return Ok(s.to_owned());
    }

    let (host, port) = parse_http_authority(s)?;
    let version = http_get_json(&host, port).await?;
    let ws = version
        .get("webSocketDebuggerUrl")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            CdpError::Connect(
                "no webSocketDebuggerUrl in /json/version (is remote debugging enabled?)".into(),
            )
        })?;
    Ok(ws.to_owned())
}

/// Extract `(host, port)` from an HTTP origin (path and scheme ignored).
fn parse_http_authority(endpoint: &str) -> Result<(String, u16)> {
    let rest = endpoint
        .strip_prefix("http://")
        .or_else(|| endpoint.strip_prefix("https://"))
        .unwrap_or(endpoint);
    // Strip any trailing path/query.
    let authority = rest.split(['/', '?']).next().unwrap_or(rest);
    let default_port = if endpoint.starts_with("https://") {
        443
    } else {
        9222
    };

    if let Some(stripped) = authority.strip_prefix('[') {
        // IPv6 literal: [::1]:9222
        let end = stripped
            .find(']')
            .ok_or_else(|| CdpError::Connect(format!("invalid endpoint `{endpoint}`")))?;
        let host = &stripped[..end];
        let after = &stripped[end + 1..];
        let port = match after.strip_prefix(':') {
            Some(p) => p
                .parse()
                .map_err(|_| CdpError::Connect(format!("invalid port in endpoint `{endpoint}`")))?,
            None => default_port,
        };
        return Ok((host.to_owned(), port));
    }

    match authority.rsplit_once(':') {
        Some((host, port_str)) => {
            let port = port_str
                .parse::<u16>()
                .map_err(|_| CdpError::Connect(format!("invalid port in endpoint `{endpoint}`")))?;
            Ok((host.to_owned(), port))
        }
        None => Ok((authority.to_owned(), default_port)),
    }
}

/// Minimal HTTP/1.1 GET used to fetch `/json/version`. No external HTTP
/// dependency is pulled in. Chromium's DevTools HTTP server keeps the
/// connection alive even with `Connection: close`, so the response body is
/// read strictly up to `Content-Length` rather than to EOF.
async fn http_get_json(host: &str, port: u16) -> Result<serde_json::Value> {
    use std::io::Cursor;

    let mut stream = tokio::net::TcpStream::connect((host, port))
        .await
        .map_err(|e| CdpError::Connect(format!("http fetch {host}:{port}: {e}")))?;

    let request = format!(
        "GET /json/version HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\nUser-Agent: sniffCSS/0.1\r\nAccept: application/json\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|e| CdpError::Connect(format!("http fetch {host}:{port}: {e}")))?;

    // Read headers up to the blank line.
    let mut header_buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match stream.read(&mut byte).await {
            Ok(0) => break,
            Ok(_) => {
                header_buf.push(byte[0]);
                if header_buf.ends_with(b"\r\n\r\n") {
                    break;
                }
                if header_buf.len() > 64 * 1024 {
                    return Err(CdpError::Connect(
                        "oversized /json/version response headers".into(),
                    ));
                }
            }
            Err(e) => return Err(CdpError::Connect(format!("http fetch {host}:{port}: {e}"))),
        }
    }
    let header_text = String::from_utf8_lossy(&header_buf);

    let content_length = parse_content_length(&header_text).ok_or_else(|| {
        CdpError::Connect("missing Content-Length in /json/version response".into())
    })?;

    let mut body = Vec::with_capacity(content_length as usize);
    stream
        .take(content_length)
        .read_to_end(&mut body)
        .await
        .map_err(|e| CdpError::Connect(format!("http fetch {host}:{port}: {e}")))?;

    let mut cursor = Cursor::new(&body);
    serde_json::from_reader(&mut cursor)
        .map_err(|e| CdpError::Connect(format!("invalid /json/version JSON: {e}")))
}

/// Extract the `Content-Length` header value (bytes).
fn parse_content_length(headers: &str) -> Option<u64> {
    headers.lines().skip(1).find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.trim().eq_ignore_ascii_case("content-length") {
            value.trim().parse().ok()
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ws_endpoints_pass_through() {
        assert_eq!(
            resolve_endpoint("ws://127.0.0.1:9222/devtools/browser/x")
                .await
                .unwrap(),
            "ws://127.0.0.1:9222/devtools/browser/x"
        );
        assert_eq!(
            resolve_endpoint("wss://browser:9222/devtools/browser/x")
                .await
                .unwrap(),
            "wss://browser:9222/devtools/browser/x"
        );
    }

    #[test]
    fn authority_parsing() {
        assert_eq!(
            parse_http_authority("127.0.0.1:9222").unwrap(),
            ("127.0.0.1".into(), 9222)
        );
        assert_eq!(
            parse_http_authority("http://host:9222").unwrap(),
            ("host".into(), 9222)
        );
        assert_eq!(parse_http_authority("host").unwrap(), ("host".into(), 9222));
        assert_eq!(
            parse_http_authority("http://host").unwrap(),
            ("host".into(), 9222)
        );
        assert_eq!(
            parse_http_authority("https://host").unwrap(),
            ("host".into(), 443)
        );
        assert_eq!(
            parse_http_authority("http://127.0.0.1:9222/json/version").unwrap(),
            ("127.0.0.1".into(), 9222)
        );
        assert_eq!(
            parse_http_authority("[::1]:9222").unwrap(),
            ("::1".into(), 9222)
        );
    }

    #[test]
    fn content_length_is_parsed() {
        assert_eq!(
            parse_content_length("HTTP/1.1 200 OK\r\nContent-Length: 405\r\nConnection: close"),
            Some(405)
        );
        assert_eq!(
            parse_content_length("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n"),
            None
        );
        assert_eq!(parse_content_length("HTTP/1.1 200 OK\r\n"), None);
    }

    #[tokio::test]
    async fn http_endpoint_is_resolved() {
        // Spin up a fake /json/version server on an ephemeral port. The
        // server keeps the socket open after replying (like Chromium does),
        // so the client must rely on Content-Length instead of EOF.
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut req = Vec::new();
            loop {
                let mut chunk = [0u8; 1024];
                match sock.read(&mut chunk).await.unwrap() {
                    0 => break,
                    n => {
                        req.extend_from_slice(&chunk[..n]);
                        if req.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }
                }
            }
            let body = r#"{"Browser":"Chromium/151.0.7922.108","webSocketDebuggerUrl":"ws://127.0.0.1:9222/devtools/browser/abc"}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = sock.write_all(resp.as_bytes()).await;
            // Keep the socket open a moment, like a keep-alive server would.
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            let _ = sock.shutdown().await;
        });

        let resolved = resolve_endpoint(&format!("http://{addr}")).await.unwrap();
        assert_eq!(resolved, "ws://127.0.0.1:9222/devtools/browser/abc");
        server.await.unwrap();
    }
}
