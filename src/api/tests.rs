//! Unit tests for the API HTTP layer: request parsing, query decoding, auth,
//! and a live loopback integration test using a mock [`EmulatorAccess`].

use std::collections::HashMap;
use std::io::Write;
use std::net::TcpStream;
use std::thread;

use serde_json::{json, Value};

use super::handlers::dispatch;
use super::{ApiServer, EmulatorAccess, HttpRequest};

/// A tiny in-memory emulator used to exercise the handlers and server.
struct MockEmu {
    mem: HashMap<u32, u8>,
    d: [u32; 8],
    a: [u32; 8],
    pc: u32,
    sr: u16,
    paused: bool,
    breakpoints: Vec<u32>,
    frame: u64,
}

impl MockEmu {
    fn new() -> Self {
        let mut mem = HashMap::new();
        // 0x7000 = MOVEQ #0,D0 (recognizable for disassembly tests).
        mem.insert(0x000000, 0x70);
        mem.insert(0x000001, 0x00);
        Self {
            mem,
            d: [0; 8],
            a: [0; 8],
            pc: 0,
            sr: 0x2700,
            paused: true,
            breakpoints: Vec::new(),
            frame: 0,
        }
    }
}

impl EmulatorAccess for MockEmu {
    fn read_byte(&mut self, addr: u32) -> u8 {
        *self.mem.get(&addr).unwrap_or(&0)
    }
    fn write_byte(&mut self, addr: u32, value: u8) {
        self.mem.insert(addr, value);
    }
    fn read_word(&mut self, addr: u32) -> u16 {
        ((self.read_byte(addr) as u16) << 8) | self.read_byte(addr.wrapping_add(1)) as u16
    }
    fn state_json(&mut self) -> Value {
        json!({
            "registers": {
                "d0": format!("0x{:08X}", self.d[0]),
                "a7": format!("0x{:08X}", self.a[7]),
                "pc": format!("0x{:06X}", self.pc),
                "sr": format!("0x{:04X}", self.sr),
                "ccr": { "c": false, "v": false, "z": false, "n": false, "x": false },
            },
            "z80": { "pc": "0x0000" },
            "frame": self.frame,
            "paused": self.paused,
            "region": "ntsc",
        })
    }
    fn write_registers(&mut self, value: &Value) -> Result<Value, String> {
        let regs = value.get("registers").unwrap_or(value);
        if let Some(pc) = regs.get("pc").and_then(parse_hex_value) {
            self.pc = pc;
        }
        if let Some(sr) = regs.get("sr").and_then(parse_hex_value) {
            self.sr = sr as u16;
        }
        if let Some(d0) = regs.get("d0").and_then(parse_hex_value) {
            self.d[0] = d0;
        }
        Ok(self
            .state_json()
            .get("registers")
            .cloned()
            .unwrap_or(Value::Null))
    }
    fn step_instruction(&mut self) {
        self.pc = self.pc.wrapping_add(2);
    }
    fn step_frame(&mut self) {
        self.frame += 1;
    }
    fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }
    fn is_paused(&self) -> bool {
        self.paused
    }
    fn reset(&mut self) {
        *self = MockEmu::new();
    }
    fn breakpoints(&self) -> Vec<u32> {
        self.breakpoints.clone()
    }
    fn add_breakpoint(&mut self, addr: u32) -> bool {
        if self.breakpoints.contains(&addr) {
            false
        } else {
            self.breakpoints.push(addr);
            true
        }
    }
    fn remove_breakpoint(&mut self, addr: u32) -> bool {
        let before = self.breakpoints.len();
        self.breakpoints.retain(|&a| a != addr);
        self.breakpoints.len() != before
    }
    fn set_input(&mut self, _value: &Value) -> Result<Value, String> {
        Ok(json!({ "applied": true }))
    }
    fn framebuffer(&mut self) -> Vec<u16> {
        vec![0xF800; 4]
    }
    fn framebuffer_width(&self) -> u32 {
        2
    }
    fn framebuffer_height(&self) -> u32 {
        2
    }
    fn save_state(&mut self, path: &str) -> Result<String, String> {
        Ok(path.to_string())
    }
    fn load_state(&mut self, path: &str) -> Result<(), String> {
        let _ = path;
        Ok(())
    }
    fn info(&self) -> Value {
        json!({
            "name": "genteel",
            "api_version": 1,
            "system": "Sega Mega Drive / Genesis",
            "endpoints": ["/state", "/registers", "/step", "/memory", "/disassemble"],
        })
    }
}

/// Parse a JSON value that may be a hex string or a number into a u32.
fn parse_hex_value(v: &Value) -> Option<u32> {
    match v {
        Value::Number(n) => n.as_u64().and_then(|x| u32::try_from(x).ok()),
        Value::String(s) => super::handlers::parse_addr(s),
        _ => None,
    }
}

/// Build an `HttpRequest` directly for pure-handler tests.
fn req(method: &str, path: &str, query: &str, body: Option<Value>) -> HttpRequest {
    HttpRequest {
        method: method.to_string(),
        path: path.to_string(),
        query: query.to_string(),
        headers: HashMap::new(),
        body: body
            .map(|b| serde_json::to_vec(&b).unwrap())
            .unwrap_or_default(),
    }
}

#[test]
fn query_param_decoding() {
    let r = req(
        "GET",
        "/memory",
        "address=0xFF0000&length=4&name=a%20b",
        None,
    );
    assert_eq!(r.query_param("address").as_deref(), Some("0xFF0000"));
    assert_eq!(r.query_param("length").as_deref(), Some("4"));
    assert_eq!(r.query_param("name").as_deref(), Some("a b"));
    assert_eq!(r.query_param("missing"), None);
}

#[test]
fn read_memory_handler() {
    let mut emu = MockEmu::new();
    emu.write_byte(0xFF0000, 0xDE);
    emu.write_byte(0xFF0001, 0xAD);
    let resp = dispatch(
        &req(
            "GET",
            "/memory",
            "address=0xFF0000&length=2&format=hex",
            None,
        ),
        &mut emu,
    );
    assert_eq!(resp.status, 200);
    let v: Value = serde_json::from_slice(&resp.body).unwrap();
    assert_eq!(v.get("hex").and_then(|h| h.as_str()), Some("dead"));
}

#[test]
fn write_memory_handler_hex() {
    let mut emu = MockEmu::new();
    let resp = dispatch(
        &req(
            "POST",
            "/memory",
            "",
            Some(json!({"address": "0xFF0000", "hex": "cafebabe"})),
        ),
        &mut emu,
    );
    assert_eq!(resp.status, 200);
    assert_eq!(emu.read_byte(0xFF0000), 0xCA);
    assert_eq!(emu.read_byte(0xFF0003), 0xBE);
}

#[test]
fn disassemble_handler() {
    let mut emu = MockEmu::new();
    emu.pc = 0;
    let resp = dispatch(
        &req("GET", "/disassemble", "address=0x0&count=1", None),
        &mut emu,
    );
    assert_eq!(resp.status, 200);
    let v: Value = serde_json::from_slice(&resp.body).unwrap();
    let first = &v["instructions"][0];
    let text = first
        .get("instruction")
        .and_then(|s| s.as_str())
        .unwrap_or("");
    assert!(text.to_uppercase().contains("MOVE"), "got: {}", text);
}

#[test]
fn step_handler_instructions() {
    let mut emu = MockEmu::new();
    emu.pc = 0x200;
    let resp = dispatch(
        &req(
            "POST",
            "/step",
            "",
            Some(json!({"type": "instruction", "count": 3})),
        ),
        &mut emu,
    );
    assert_eq!(resp.status, 200);
    assert_eq!(emu.pc, 0x200 + 3 * 2);
}

#[test]
fn step_handler_rejects_huge_count() {
    let mut emu = MockEmu::new();
    let resp = dispatch(
        &req(
            "POST",
            "/step",
            "",
            Some(json!({"type": "instruction", "count": 99999999})),
        ),
        &mut emu,
    );
    assert_eq!(resp.status, 400);
}

#[test]
fn unknown_route_is_404() {
    let mut emu = MockEmu::new();
    let resp = dispatch(&req("GET", "/nope", "", None), &mut emu);
    assert_eq!(resp.status, 404);
}

#[test]
fn breakpoints_add_remove() {
    let mut emu = MockEmu::new();
    let resp = dispatch(
        &req(
            "POST",
            "/breakpoints",
            "",
            Some(json!({"address": "0x1234"})),
        ),
        &mut emu,
    );
    assert_eq!(resp.status, 200);
    assert!(!emu.add_breakpoint(0x1234)); // already present
    let resp = dispatch(
        &req(
            "DELETE",
            "/breakpoints",
            "",
            Some(json!({"address": "0x1234"})),
        ),
        &mut emu,
    );
    assert_eq!(resp.status, 200);
    assert!(emu.breakpoints().is_empty());
}

/// Live end-to-end test: start the server on an ephemeral loopback port,
/// connect a real TCP client, and exercise several endpoints over HTTP.
#[test]
fn live_server_roundtrip() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let mut server = ApiServer::new(0, None, "127.0.0.1").expect("bind");
    let port = server.port();
    assert_ne!(port, 0);
    server.set_nonblocking(true).expect("nonblocking");

    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = Arc::clone(&stop);
    let handle = thread::spawn(move || {
        let mut emu = MockEmu::new();
        emu.write_byte(0xFF0000, 0x42);
        while !stop_clone.load(Ordering::Relaxed) {
            match server.try_accept() {
                Ok(Some(stream)) => {
                    let _ = server.serve_client(stream, dispatch, &mut emu);
                }
                _ => std::thread::sleep(std::time::Duration::from_millis(5)),
            }
        }
    });

    let body = http_get(port, "/info", None);
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v.get("api_version").and_then(|x| x.as_u64()), Some(1));

    let body = http_get(port, "/state", None);
    let v: Value = serde_json::from_str(&body).unwrap();
    assert!(v.get("registers").is_some());

    let body = http_request(
        port,
        "POST",
        "/step",
        Some(&json!({"type":"instruction","count":2})),
        None,
    );
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v.get("count").and_then(|x| x.as_u64()), Some(2));

    let body = http_get(port, "/disassemble?address=0x0&count=1", None);
    let v: Value = serde_json::from_str(&body).unwrap();
    assert!(v.get("instructions").is_some());

    stop.store(true, Ordering::Relaxed);
    handle.join().ok();
}

/// Auth test: a token-protected server rejects unauthenticated requests.
#[test]
fn auth_gate_rejects_without_token() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let mut server = ApiServer::new(0, Some("s3cr3t".to_string()), "127.0.0.1").expect("bind");
    let port = server.port();
    let token = "s3cr3t".to_string();
    server.set_nonblocking(true).expect("nonblocking");

    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = Arc::clone(&stop);
    let handle = thread::spawn(move || {
        let mut emu = MockEmu::new();
        while !stop_clone.load(Ordering::Relaxed) {
            match server.try_accept() {
                Ok(Some(stream)) => {
                    let _ = server.serve_client(stream, dispatch, &mut emu);
                }
                _ => std::thread::sleep(std::time::Duration::from_millis(5)),
            }
        }
    });

    // Without token -> 401.
    let raw = http_get_raw(port, "/state", None);
    assert!(
        raw.starts_with("HTTP/1.1 401"),
        "got: {}",
        raw.lines().next().unwrap_or("")
    );

    // With token -> 200.
    let raw = http_get_raw(port, "/state", Some(&token));
    assert!(
        raw.starts_with("HTTP/1.1 200"),
        "got: {}",
        raw.lines().next().unwrap_or("")
    );

    stop.store(true, Ordering::Relaxed);
    handle.join().ok();
}

/// Perform an HTTP GET and return the response body as a string.
fn http_get(port: u16, path: &str, auth: Option<&str>) -> String {
    http_request(port, "GET", path, None, auth)
}

/// Perform a raw HTTP request and return the response body as a string.
fn http_request(
    port: u16,
    method: &str,
    path: &str,
    body: Option<&Value>,
    auth: Option<&str>,
) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let body_bytes = body
        .map(|b| serde_json::to_vec(b).unwrap())
        .unwrap_or_default();
    let mut request = format!("{} {} HTTP/1.1\r\nHost: 127.0.0.1\r\n", method, path);
    if let Some(a) = auth {
        request.push_str(&format!("Authorization: Bearer {}\r\n", a));
    }
    if !body_bytes.is_empty() {
        request.push_str(&format!("Content-Length: {}\r\n", body_bytes.len()));
    }
    request.push_str("Connection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).unwrap();
    stream.write_all(&body_bytes).unwrap();
    stream.flush().unwrap();

    read_response_body(stream)
}

/// Read exactly the bytes advertised by `Content-Length` (falling back to
/// reading to EOF). Returns only the body, matching real HTTP client behavior
/// so the server's keep-alive read loop sees a prompt EOF instead of blocking
/// until its read timeout.
fn read_response_body(mut stream: TcpStream) -> String {
    use std::io::Read;
    let mut raw = Vec::new();
    // Read the header block first.
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos;
        }
        let n = stream.read(&mut chunk).unwrap_or(0);
        if n == 0 {
            break raw.len();
        }
        raw.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&raw[..header_end.min(raw.len())]).into_owned();
    let content_length: usize = head
        .lines()
        .find_map(|l| {
            let l = l.to_ascii_lowercase();
            l.strip_prefix("content-length:")
                .map(|v| v.trim().to_string())
        })
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let mut body = raw[header_end + 4..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut chunk).unwrap_or(0);
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    String::from_utf8_lossy(&body).into_owned()
}

/// Raw HTTP GET returning the entire response (status line + headers + body).
fn http_get_raw(port: u16, path: &str, auth: Option<&str>) -> String {
    use std::io::Read;
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let mut request = format!("GET {} HTTP/1.1\r\nHost: 127.0.0.1\r\n", path);
    if let Some(a) = auth {
        request.push_str(&format!("Authorization: Bearer {}\r\n", a));
    }
    request.push_str("Connection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).unwrap();
    stream.flush().unwrap();
    // Read until EOF (the server closes after the 30s read timeout on this
    // unauthenticated probe path); bounded by the stream read so the test
    // does not hang indefinitely if the server misbehaves.
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .ok();
    let mut resp = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => resp.extend_from_slice(&chunk[..n]),
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&resp).into_owned()
}
