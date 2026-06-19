//! The single **write** to NeuroSkill: fire its local `label` WebSocket command.
//!
//! Hand-rolled over `tokio::net::TcpStream` (no WS SDK), the same raw-socket style
//! as the Gmail OAuth loopback in `commands.rs`. Best-effort: connect, perform the
//! WS upgrade handshake, send one masked text frame, send a close frame, drop. We
//! never write to `labels.sqlite` directly — the daemon owns that file; the
//! `label` command is the sanctioned path.

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Fire `{"command":"label","text":<text>}` at the NeuroSkill WebSocket. Returns
/// `Err` (a friendly message) if the daemon isn't reachable — the caller surfaces
/// that as an info toast and never blocks the launch on it.
pub async fn fire_label(ws_url: &str, text: &str) -> Result<(), String> {
    let (host, port, path) = parse_ws_url(ws_url)?;
    let payload = serde_json::json!({ "command": "label", "text": text }).to_string();
    let addr = format!("{host}:{port}");

    let mut stream = TcpStream::connect(&addr)
        .await
        .map_err(|e| format!("NeuroSkill isn't reachable at {addr} (is the daemon running?): {e}"))?;

    // --- WS opening handshake ---
    let key = ws_key()?;
    let req = format!(
        "GET {path} HTTP/1.1\r\n\
Host: {host}:{port}\r\n\
Upgrade: websocket\r\n\
Connection: Upgrade\r\n\
Sec-WebSocket-Key: {key}\r\n\
Sec-WebSocket-Version: 13\r\n\r\n"
    );
    stream
        .write_all(req.as_bytes())
        .await
        .map_err(|e| format!("WebSocket handshake write failed: {e}"))?;

    // Read just the response headers (up to the blank line). We don't verify the
    // Sec-WebSocket-Accept hash — confirming the 101 status is enough for a
    // best-effort local fire.
    let mut buf = Vec::new();
    let mut tmp = [0u8; 1024];
    loop {
        let n = stream
            .read(&mut tmp)
            .await
            .map_err(|e| format!("WebSocket handshake read failed: {e}"))?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") || buf.len() > 8192 {
            break;
        }
    }
    let head = String::from_utf8_lossy(&buf);
    let status = head.lines().next().unwrap_or_default();
    if !status.contains("101") {
        return Err(format!(
            "NeuroSkill rejected the WebSocket upgrade ({}).",
            status.trim()
        ));
    }

    // --- One masked text frame, then a close frame (both client→server frames
    // MUST be masked per RFC 6455). ---
    let frame = encode_frame(0x1, payload.as_bytes())?;
    stream
        .write_all(&frame)
        .await
        .map_err(|e| format!("WebSocket frame write failed: {e}"))?;
    let close = encode_frame(0x8, &[])?;
    let _ = stream.write_all(&close).await;
    let _ = stream.flush().await;
    Ok(())
}

/// Split a `ws://host:port/path` URL into `(host, port, path)`. `wss://` is
/// rejected (the daemon is plaintext localhost). A missing port defaults to
/// NeuroSkill's 8375; a missing path to `/`. Pure — unit-tested.
fn parse_ws_url(url: &str) -> Result<(String, u16, String), String> {
    let rest = url
        .strip_prefix("ws://")
        .ok_or_else(|| format!("NeuroSkill ws_url must start with ws:// (got \"{url}\")."))?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (
            h.to_string(),
            p.parse::<u16>()
                .map_err(|_| format!("invalid port in ws_url \"{url}\"."))?,
        ),
        None => (authority.to_string(), 8375),
    };
    if host.is_empty() {
        return Err(format!("missing host in ws_url \"{url}\"."));
    }
    Ok((host, port, path.to_string()))
}

/// A random 16-byte `Sec-WebSocket-Key` (standard base64).
fn ws_key() -> Result<String, String> {
    use base64::Engine;
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).map_err(|e| format!("could not gather entropy: {e}"))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(b))
}

/// Encode one final, masked client frame with the given opcode (0x1 text, 0x8
/// close). Pure given the mask; the random mask is the only nondeterminism.
fn encode_frame(opcode: u8, payload: &[u8]) -> Result<Vec<u8>, String> {
    let mut mask = [0u8; 4];
    getrandom::fill(&mut mask).map_err(|e| format!("could not gather entropy: {e}"))?;

    let mut frame = Vec::with_capacity(payload.len() + 8);
    frame.push(0x80 | (opcode & 0x0f)); // FIN + opcode
    let len = payload.len();
    if len < 126 {
        frame.push(0x80 | len as u8); // MASK bit + 7-bit length
    } else if len <= u16::MAX as usize {
        frame.push(0x80 | 126);
        frame.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        frame.push(0x80 | 127);
        frame.extend_from_slice(&(len as u64).to_be_bytes());
    }
    frame.extend_from_slice(&mask);
    for (i, b) in payload.iter().enumerate() {
        frame.push(b ^ mask[i % 4]);
    }
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ws_urls() {
        assert_eq!(
            parse_ws_url("ws://127.0.0.1:8375").unwrap(),
            ("127.0.0.1".to_string(), 8375, "/".to_string())
        );
        assert_eq!(
            parse_ws_url("ws://localhost:9000/labels").unwrap(),
            ("localhost".to_string(), 9000, "/labels".to_string())
        );
        // Missing port → NeuroSkill default.
        assert_eq!(
            parse_ws_url("ws://host.example").unwrap(),
            ("host.example".to_string(), 8375, "/".to_string())
        );
        // wss / bad port / empty host rejected.
        assert!(parse_ws_url("wss://127.0.0.1:8375").is_err());
        assert!(parse_ws_url("ws://127.0.0.1:notaport").is_err());
        assert!(parse_ws_url("http://127.0.0.1").is_err());
    }

    #[test]
    fn frame_is_masked_and_roundtrips() {
        let payload = b"{\"command\":\"label\",\"text\":\"waid:brief=x:start\"}";
        let frame = encode_frame(0x1, payload).unwrap();
        // FIN + text opcode.
        assert_eq!(frame[0], 0x81);
        // MASK bit set; 7-bit length is the payload length (< 126).
        assert_eq!(frame[1] & 0x80, 0x80);
        assert_eq!((frame[1] & 0x7f) as usize, payload.len());
        // Unmask the body and recover the payload.
        let mask = &frame[2..6];
        let body = &frame[6..];
        let unmasked: Vec<u8> = body.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]).collect();
        assert_eq!(unmasked, payload);
    }

    #[test]
    fn extended_length_frame() {
        let payload = vec![b'a'; 200]; // >= 126 → 16-bit extended length
        let frame = encode_frame(0x1, &payload).unwrap();
        assert_eq!(frame[1] & 0x7f, 126);
        let declared = u16::from_be_bytes([frame[2], frame[3]]) as usize;
        assert_eq!(declared, 200);
    }
}
