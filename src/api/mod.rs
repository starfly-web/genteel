//! Agent-facing JSON/HTTP API for the Genteel emulator.
//!
//! This module exposes a small, dependency-free HTTP/1.1 server (built on
//! `std::net`) that lets external tools — in particular AI agents such as
//! Cline or Kilo — drive the emulator for reverse engineering: read/write
//! memory and registers, single-step, disassemble code, manage breakpoints,
//! inject controller input, and snapshot state.
//!
//! ## Design constraints
//!
//! * The emulator state lives behind `Rc<RefCell<Bus>>`, which is **not**
//!   `Send`/`Sync`. Therefore the server is intentionally *single-threaded*
//!   and interleaved with emulation by the run loop (mirroring the existing
//!   GDB stub in [`crate::debugger`]). It is never run on a background thread
//!   that shares the emulator.
//! * Only minimal HTTP/1.1 is implemented: a request line, headers, an
//!   optional body, and JSON responses with a `Content-Length`. This is
//!   enough for programmatic clients and avoids pulling in an async runtime.
//! * For safety the listener binds to loopback (`127.0.0.1`) by default and
//!   supports an optional shared-secret token checked in constant time.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use serde_json::{json, Value};

pub mod handlers;

/// Default API server port.
pub const DEFAULT_API_PORT: u16 = 4545;

/// Maximum size (in bytes) of a single HTTP request we will buffer.
const MAX_REQUEST_SIZE: usize = 1024 * 1024; // 1 MiB

/// Maximum number of instructions/frames a single `/step` may request.
const MAX_STEP_COUNT: u32 = 100_000;

/// Maximum number of bytes a single `/memory` read may request.
const MAX_MEMORY_READ: usize = 64 * 1024; // 64 KiB

/// Maximum number of instructions a single `/disassemble` may request.
const MAX_DISASM_COUNT: u32 = 4096;

/// A parsed HTTP request (only what the API needs).
#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: String,
    /// Path without the query string, e.g. `/memory`.
    pub path: String,
    /// Raw query string without the leading `?`, if any.
    pub query: String,
    /// Lower-cased header map.
    pub headers: HashMap<String, String>,
    /// Raw request body bytes.
    pub body: Vec<u8>,
}

impl HttpRequest {
    /// Parse a query parameter value (percent + `+` decoding, first match).
    pub fn query_param(&self, key: &str) -> Option<String> {
        parse_query(&self.query).remove(key)
    }

    /// Body decoded as a UTF-8 string (lossy).
    pub fn body_str(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// Body parsed as a JSON object/value. An empty body yields an empty
    /// object so handlers can treat "no body" uniformly.
    pub fn json_body(&self) -> Option<Value> {
        if self.body.is_empty() {
            return Some(Value::Object(serde_json::Map::new()));
        }
        serde_json::from_slice(&self.body).ok()
    }
}

/// A minimal HTTP response carrying a JSON body.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub status_text: &'static str,
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// Build a JSON response with the given status code.
    pub fn json(status: u16, value: &Value) -> Self {
        let body = serde_json::to_vec(value).unwrap_or_else(|_| b"{}".to_vec());
        Self {
            status,
            status_text: status_text(status),
            body,
        }
    }

    /// Convenience: `200 OK` with a JSON body.
    pub fn ok(value: &Value) -> Self {
        Self::json(200, value)
    }

    /// Convenience: JSON error body `{"error": "..."}` with a status code.
    pub fn error(status: u16, message: &str) -> Self {
        Self::json(status, &json!({ "error": message }))
    }

    /// Serialize to bytes ready to write to the socket.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.body.len() + 160);
        out.extend_from_slice(
            format!("HTTP/1.1 {} {}\r\n", self.status, self.status_text).as_bytes(),
        );
        out.extend_from_slice(b"Content-Type: application/json\r\n");
        out.extend_from_slice(b"Access-Control-Allow-Origin: *\r\n");
        out.extend_from_slice(b"Cache-Control: no-store\r\n");
        out.extend_from_slice(format!("Content-Length: {}\r\n", self.body.len()).as_bytes());
        out.extend_from_slice(b"Connection: close\r\n");
        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(&self.body);
        out
    }
}

/// Map an HTTP status code to its canonical reason phrase.
fn status_text(code: u16) -> &'static str {
    match code {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        _ => "OK",
    }
}

/// Percent- and `+`-decode a query string into a key/value map.
fn parse_query(query: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = match pair.split_once('=') {
            Some((k, v)) => (k, v),
            None => (pair, ""),
        };
        map.insert(percent_decode(k), percent_decode(v));
    }
    map
}

/// Decode `application/x-www-form-urlencoded` style percent-encoding.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hi = hex_val(bytes[i + 1]);
                let lo = hex_val(bytes[i + 2]);
                match (hi, lo) {
                    (Some(h), Some(l)) => {
                        out.push((h << 4) | l);
                        i += 3;
                    }
                    _ => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Read a full HTTP request from a blocking stream.
///
/// Returns `Ok(None)` on a clean connection close before any bytes arrive.
fn read_request(stream: &mut TcpStream) -> std::io::Result<Option<HttpRequest>> {
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];

    // Read until we have the full header block (terminated by CRLFCRLF).
    let header_end = loop {
        if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
            break pos;
        }
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            if buf.is_empty() {
                return Ok(None);
            }
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "connection closed mid-headers",
            ));
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.len() > MAX_REQUEST_SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "request headers too large",
            ));
        }
    };

    let head = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let mut lines = head.split("\r\n");

    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_ascii_uppercase();
    let target = parts.next().unwrap_or("/").to_string();
    if method.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "missing request method",
        ));
    }

    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target, String::new()),
    };

    let mut headers = HashMap::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }

    // Determine body length.
    let content_length: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    if content_length > MAX_REQUEST_SIZE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "request body too large",
        ));
    }

    // Whatever bytes we already read past the header block belong to the body.
    let mut body = buf[header_end + 4..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(content_length);

    Ok(Some(HttpRequest {
        method,
        path,
        query,
        headers,
        body,
    }))
}

/// Find the first occurrence of `needle` in `haystack`.
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Constant-time comparison for the API token. Not security-critical on a
/// loopback socket, but we mirror the GDB stub's approach of using the
/// `subtle` crate to avoid early-exit timing differences.
fn token_matches(provided: &[u8], expected: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    if provided.len() != expected.len() {
        return false;
    }
    provided.ct_eq(expected).into()
}

/// Abstraction over the emulator so the API handlers can be unit-tested with
/// a mock, and so `serve_client` does not need to name the concrete
/// `Emulator` type (which lives in the binary crate, not the library).
///
/// The concrete implementation lives in the binary (`main.rs`) and forwards
/// to the real `Emulator`.
pub trait EmulatorAccess {
    /// Read a byte from the 68k bus.
    fn read_byte(&mut self, addr: u32) -> u8;
    /// Write a byte to the 68k bus.
    fn write_byte(&mut self, addr: u32, value: u8);
    /// Read a big-endian 16-bit word from the 68k bus.
    fn read_word(&mut self, addr: u32) -> u16;
    /// Full machine state as JSON.
    fn state_json(&mut self) -> Value;
    /// Apply register writes described by a JSON object.
    fn write_registers(&mut self, value: &Value) -> Result<Value, String>;
    /// Step one M68k instruction.
    fn step_instruction(&mut self);
    /// Run one full frame.
    fn step_frame(&mut self);
    /// Set the paused flag.
    fn set_paused(&mut self, paused: bool);
    /// Whether the machine is currently paused.
    fn is_paused(&self) -> bool;
    /// Hard reset the machine.
    fn reset(&mut self);
    /// Current breakpoint set.
    fn breakpoints(&self) -> Vec<u32>;
    /// Add a breakpoint; returns whether it was newly added.
    fn add_breakpoint(&mut self, addr: u32) -> bool;
    /// Remove a breakpoint; returns whether it existed.
    fn remove_breakpoint(&mut self, addr: u32) -> bool;
    /// Inject a controller input frame from JSON.
    fn set_input(&mut self, value: &Value) -> Result<Value, String>;
    /// Framebuffer as RGB565 `u16` values.
    fn framebuffer(&mut self) -> Vec<u16>;
    /// Framebuffer width in pixels.
    fn framebuffer_width(&self) -> u32;
    /// Framebuffer height in pixels.
    fn framebuffer_height(&self) -> u32;
    /// Save state to a file path; returns the path on success.
    fn save_state(&mut self, path: &str) -> Result<String, String>;
    /// Load state from a file path.
    fn load_state(&mut self, path: &str) -> Result<(), String>;
    /// Capabilities/version descriptor.
    fn info(&self) -> Value;
}

/// The agent-facing API server.
///
/// Holds only networking/auth state; the emulator is borrowed per-request by
/// [`crate::api::handlers::dispatch`]. See the module docs for the threading
/// rationale.
pub struct ApiServer {
    listener: TcpListener,
    /// Optional shared secret. `None` means "no auth required" (loopback only).
    token: Option<String>,
}

impl ApiServer {
    /// Create a new API server bound to `bind` (e.g. `127.0.0.1`) on `port`.
    ///
    /// If `token` is `Some`, clients must send it as an `Authorization:
    /// Bearer <token>` header. Pass `None` for no auth (loopback only).
    pub fn new(port: u16, token: Option<String>, bind: &str) -> std::io::Result<Self> {
        let addr = format!("{}:{}", bind, port);
        let listener = TcpListener::bind(&addr)?;

        match &token {
            Some(_) => println!(
                "🤖 API server listening on http://{}/  (bearer token required)",
                addr
            ),
            None => println!("🤖 API server listening on http://{}/  (no auth)", addr),
        }

        Ok(Self { listener, token })
    }

    /// Switch the listener between blocking and non-blocking accept modes.
    pub fn set_nonblocking(&mut self, nonblocking: bool) -> std::io::Result<()> {
        self.listener.set_nonblocking(nonblocking)
    }

    /// The local port actually bound (useful when `port` was `0`).
    pub fn port(&self) -> u16 {
        self.listener
            .local_addr()
            .map(|a| a.port())
            .unwrap_or_default()
    }

    /// Block until a client connects and return the stream.
    pub fn accept(&self) -> std::io::Result<TcpStream> {
        let (stream, _addr) = self.listener.accept()?;
        Ok(stream)
    }

    /// Non-blocking accept used by the run loop. Returns `Ok(None)` when no
    /// client is currently waiting.
    pub fn try_accept(&self) -> std::io::Result<Option<TcpStream>> {
        match self.listener.accept() {
            Ok((stream, _)) => Ok(Some(stream)),
            Err(ref e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    /// Whether auth is required for this server.
    pub fn requires_auth(&self) -> bool {
        self.token.is_some()
    }

    /// Validate the `Authorization` header of a request against our token.
    pub fn authorize(&self, req: &HttpRequest) -> bool {
        let Some(expected) = &self.token else {
            return true;
        };
        let Some(header) = req.headers.get("authorization") else {
            return false;
        };
        // Accept "Bearer <token>" or the raw token.
        let provided = header
            .strip_prefix("Bearer ")
            .or_else(|| header.strip_prefix("bearer "))
            .unwrap_or(header.as_str());
        token_matches(provided.as_bytes(), expected.as_bytes())
    }

    /// Handle a single connected client: read requests and write responses
    /// until the client disconnects. `dispatch` is invoked per request with a
    /// borrowed emulator, so this stays single-threaded and `!Send`-safe.
    ///
    /// HTTP keep-alive is supported: the connection is reused until the client
    /// sends `Connection: close` (or closes the socket), at which point we
    /// stop. This avoids holding sockets open until the read timeout fires.
    pub fn serve_client(
        &self,
        mut stream: TcpStream,
        dispatch: impl Fn(&HttpRequest, &mut dyn EmulatorAccess) -> HttpResponse,
        emu: &mut dyn EmulatorAccess,
    ) -> std::io::Result<()> {
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;
        stream.set_nodelay(true).ok();
        loop {
            match read_request(&mut stream) {
                Ok(Some(req)) => {
                    let conn_close = req
                        .headers
                        .get("connection")
                        .map(|v| v.eq_ignore_ascii_case("close"))
                        .unwrap_or(false);

                    // Auth gate (except a friendly root/health probe).
                    if self.requires_auth()
                        && !(req.method == "GET" && (req.path == "/" || req.path == "/info"))
                        && !self.authorize(&req)
                    {
                        let resp = HttpResponse::error(401, "unauthorized");
                        stream.write_all(&resp.to_bytes())?;
                        stream.flush()?;
                        if conn_close {
                            break;
                        }
                        continue;
                    }
                    let resp = dispatch(&req, emu);
                    stream.write_all(&resp.to_bytes())?;
                    stream.flush()?;
                    if conn_close {
                        break;
                    }
                }
                Ok(None) => break, // clean close
                Err(_) => break,   // malformed / timed out / too large
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
