//! HTTP request routing and per-endpoint handlers for the agent API.
//!
//! Each handler receives the parsed [`HttpRequest`] and a `&mut dyn
//! EmulatorAccess` and returns an [`HttpResponse`]. The router
//! ([`dispatch`]) matches on method + path and produces JSON responses.

use serde_json::{json, Map, Value};

use super::{HttpRequest, HttpResponse, MAX_DISASM_COUNT, MAX_MEMORY_READ, MAX_STEP_COUNT};

/// Parse a hex/decimal address from a query parameter or JSON field.
///
/// An explicit `0x`/`$` prefix forces hexadecimal; a bare number is parsed as
/// decimal first and falls back to hex (so `"255"` is 255 but `"FF0000"` is
/// `0xFF0000`). Returns `None` on parse failure.
pub fn parse_addr(s: &str) -> Option<u32> {
    let t = s.trim();
    if let Some(hex) = t
        .strip_prefix("0x")
        .or_else(|| t.strip_prefix("0X"))
        .or_else(|| t.strip_prefix('$'))
    {
        return u32::from_str_radix(hex, 16).ok();
    }
    // Bare number: prefer decimal, then hex.
    t.parse::<u32>()
        .ok()
        .or_else(|| u32::from_str_radix(t, 16).ok())
}

/// Extract a `u32` from a JSON value that may be a number or a hex string.
fn json_to_u32(v: &Value) -> Option<u32> {
    match v {
        Value::Number(n) => n.as_u64().and_then(|x| u32::try_from(x).ok()),
        Value::String(s) => parse_addr(s),
        _ => None,
    }
}

/// Route a request to the appropriate handler.
pub fn dispatch(req: &HttpRequest, emu: &mut dyn crate::api::EmulatorAccess) -> HttpResponse {
    let path = req.path.trim_end_matches('/');
    let path = if path.is_empty() { "/" } else { path };

    match (req.method.as_str(), path) {
        ("GET", "/") | ("GET", "/info") => handle_info(emu),
        ("GET", "/state") => handle_state(emu),
        ("GET", "/registers") => handle_get_registers(emu),
        ("POST", "/registers") => handle_write_registers(req, emu),
        ("POST", "/step") => handle_step(req, emu),
        ("POST", "/run") => {
            emu.set_paused(false);
            HttpResponse::ok(&json!({ "running": true }))
        }
        ("POST", "/pause") => {
            emu.set_paused(true);
            HttpResponse::ok(&json!({ "running": false }))
        }
        ("POST", "/reset") => {
            emu.reset();
            HttpResponse::ok(&json!({ "reset": true }))
        }
        ("GET", "/memory") => handle_read_memory(req, emu),
        ("POST", "/memory") => handle_write_memory(req, emu),
        ("GET", "/disassemble") => handle_disassemble(req, emu),
        ("GET", "/breakpoints") => handle_list_breakpoints(emu),
        ("POST", "/breakpoints") => handle_add_breakpoint(req, emu),
        ("DELETE", "/breakpoints") => handle_remove_breakpoint(req, emu),
        ("POST", "/input") => handle_input(req, emu),
        ("GET", "/framebuffer") => handle_framebuffer(emu),
        ("POST", "/savestate/save") => handle_save_state(req, emu),
        ("POST", "/savestate/load") => handle_load_state(req, emu),
        _ => HttpResponse::error(404, &format!("no route for {} {}", req.method, path)),
    }
}

fn handle_info(emu: &mut dyn crate::api::EmulatorAccess) -> HttpResponse {
    let mut info = emu.info();
    if let Value::Object(ref mut m) = info {
        m.insert("paused".into(), json!(emu.is_paused()));
    }
    HttpResponse::ok(&info)
}

fn handle_state(emu: &mut dyn crate::api::EmulatorAccess) -> HttpResponse {
    HttpResponse::ok(&emu.state_json())
}

fn handle_get_registers(emu: &mut dyn crate::api::EmulatorAccess) -> HttpResponse {
    let state = emu.state_json();
    HttpResponse::ok(
        &json!({ "registers": state.get("registers").cloned().unwrap_or(Value::Null) }),
    )
}

fn handle_write_registers(
    req: &HttpRequest,
    emu: &mut dyn crate::api::EmulatorAccess,
) -> HttpResponse {
    let Some(body) = req.json_body() else {
        return HttpResponse::error(400, "invalid JSON body");
    };
    match emu.write_registers(&body) {
        Ok(updated) => HttpResponse::ok(&json!({ "registers": updated })),
        Err(e) => HttpResponse::error(400, &e),
    }
}

fn handle_step(req: &HttpRequest, emu: &mut dyn crate::api::EmulatorAccess) -> HttpResponse {
    // Defaults: one instruction.
    let mut kind = "instruction".to_string();
    let mut count: u32 = 1;

    if let Some(body) = req.json_body() {
        if let Some(t) = body.get("type").and_then(|v| v.as_str()) {
            kind = t.to_string();
        }
        if let Some(c) = body.get("count").and_then(|v| v.as_u64()) {
            count = u32::try_from(c).unwrap_or(1);
        }
    }
    // Allow query-param overrides: /step?type=frame&count=10
    if let Some(t) = req.query_param("type") {
        kind = t;
    }
    if let Some(c) = req.query_param("count").and_then(|c| c.parse::<u32>().ok()) {
        count = c;
    }

    if count == 0 {
        return HttpResponse::error(400, "count must be >= 1");
    }
    if count > MAX_STEP_COUNT {
        return HttpResponse::error(400, &format!("count exceeds limit of {}", MAX_STEP_COUNT));
    }

    match kind.as_str() {
        "instruction" | "instructions" | "step" => {
            for _ in 0..count {
                emu.step_instruction();
            }
        }
        "frame" | "frames" => {
            for _ in 0..count {
                emu.step_frame();
            }
        }
        other => return HttpResponse::error(400, &format!("unknown step type '{}'", other)),
    }

    HttpResponse::ok(&json!({ "stepped": kind, "count": count, "state": emu.state_json() }))
}

fn handle_read_memory(req: &HttpRequest, emu: &mut dyn crate::api::EmulatorAccess) -> HttpResponse {
    let Some(addr) = req.query_param("address").as_deref().and_then(parse_addr) else {
        return HttpResponse::error(400, "missing or invalid 'address' query parameter");
    };
    let length = req
        .query_param("length")
        .and_then(|l| l.parse::<usize>().ok())
        .unwrap_or(16);
    if length == 0 {
        return HttpResponse::error(400, "length must be >= 1");
    }
    if length > MAX_MEMORY_READ {
        return HttpResponse::error(
            400,
            &format!("length exceeds limit of {} bytes", MAX_MEMORY_READ),
        );
    }

    let mut bytes = Vec::with_capacity(length);
    for i in 0..length {
        bytes.push(emu.read_byte(addr.wrapping_add(i as u32)));
    }

    let format = req.query_param("format").unwrap_or_else(|| "both".into());
    let mut obj = Map::new();
    obj.insert("address".into(), json!(format!("0x{:06X}", addr)));
    obj.insert("length".into(), json!(bytes.len()));
    if format == "bytes" || format == "both" {
        obj.insert("bytes".into(), json!(bytes));
    }
    if format == "hex" || format == "both" {
        obj.insert("hex".into(), json!(hex_string(&bytes)));
    }
    if format == "ascii" || format == "both" {
        obj.insert("ascii".into(), json!(ascii_string(&bytes)));
    }
    HttpResponse::ok(&Value::Object(obj))
}

fn handle_write_memory(
    req: &HttpRequest,
    emu: &mut dyn crate::api::EmulatorAccess,
) -> HttpResponse {
    let Some(body) = req.json_body() else {
        return HttpResponse::error(400, "invalid JSON body");
    };
    let Some(addr) = body.get("address").and_then(json_to_u32) else {
        return HttpResponse::error(400, "missing 'address'");
    };

    let bytes: Option<Vec<u8>> = if let Some(arr) = body.get("bytes").and_then(|v| v.as_array()) {
        let mut out = Vec::with_capacity(arr.len());
        for v in arr {
            match v.as_u64().and_then(|x| u8::try_from(x).ok()) {
                Some(b) => out.push(b),
                None => return HttpResponse::error(400, "'bytes' must be an array of 0..=255"),
            }
        }
        Some(out)
    } else if let Some(hex) = body.get("hex").and_then(|v| v.as_str()) {
        parse_hex(hex)
    } else {
        None
    };

    let Some(bytes) = bytes else {
        return HttpResponse::error(400, "missing 'bytes' or 'hex'");
    };
    if bytes.len() > MAX_MEMORY_READ {
        return HttpResponse::error(
            400,
            &format!("write exceeds limit of {} bytes", MAX_MEMORY_READ),
        );
    }

    for (i, b) in bytes.iter().enumerate() {
        emu.write_byte(addr.wrapping_add(i as u32), *b);
    }

    HttpResponse::ok(&json!({
        "address": format!("0x{:06X}", addr),
        "written": bytes.len(),
    }))
}

fn handle_disassemble(req: &HttpRequest, emu: &mut dyn crate::api::EmulatorAccess) -> HttpResponse {
    // Default to the current PC when no address is supplied.
    let addr = match req.query_param("address").as_deref().and_then(parse_addr) {
        Some(a) => a,
        None => {
            let state = emu.state_json();
            match state
                .pointer("/registers/pc")
                .and_then(|v| v.as_str())
                .and_then(parse_addr)
            {
                Some(pc) => pc,
                None => {
                    return HttpResponse::error(400, "missing or invalid 'address' query parameter")
                }
            }
        }
    };
    let count = req
        .query_param("count")
        .and_then(|c| c.parse::<u32>().ok())
        .unwrap_or(10)
        .clamp(1, MAX_DISASM_COUNT);
    let syntax = req
        .query_param("syntax")
        .unwrap_or_else(|| "motorola".into());
    let gnu = syntax.eq_ignore_ascii_case("gnu") || syntax.eq_ignore_ascii_case("att");

    let mut lines = Vec::with_capacity(count as usize);
    let mut cur = addr;
    for _ in 0..count {
        let opcode = emu.read_word(cur);
        let instr = crate::cpu::decode(opcode);
        let words = instr.length_words().min(8);
        let mut raw = Vec::with_capacity(words as usize);
        for i in 0..words {
            raw.push(emu.read_word(cur.wrapping_add(i * 2)));
        }
        let text = instr.format_with_words(cur, &raw, gnu);
        lines.push(json!({
            "address": format!("0x{:06X}", cur),
            "bytes": raw.iter().map(|w| format!("{:04X}", w)).collect::<Vec<_>>(),
            "instruction": text,
        }));
        if words == 0 {
            break;
        }
        cur = cur.wrapping_add(words * 2);
    }

    HttpResponse::ok(&json!({
        "address": format!("0x{:06X}", addr),
        "syntax": if gnu { "gnu" } else { "motorola" },
        "count": lines.len(),
        "instructions": lines,
    }))
}

fn handle_list_breakpoints(emu: &mut dyn crate::api::EmulatorAccess) -> HttpResponse {
    let bps: Vec<String> = emu
        .breakpoints()
        .into_iter()
        .map(|a| format!("0x{:06X}", a))
        .collect();
    HttpResponse::ok(&json!({ "breakpoints": bps }))
}

fn handle_add_breakpoint(
    req: &HttpRequest,
    emu: &mut dyn crate::api::EmulatorAccess,
) -> HttpResponse {
    let Some(body) = req.json_body() else {
        return HttpResponse::error(400, "invalid JSON body");
    };
    let Some(addr) = body.get("address").and_then(json_to_u32) else {
        return HttpResponse::error(400, "missing 'address'");
    };
    let added = emu.add_breakpoint(addr);
    HttpResponse::ok(&json!({
        "address": format!("0x{:06X}", addr),
        "added": added,
        "breakpoints": emu.breakpoints().iter().map(|a| format!("0x{:06X}", a)).collect::<Vec<_>>(),
    }))
}

fn handle_remove_breakpoint(
    req: &HttpRequest,
    emu: &mut dyn crate::api::EmulatorAccess,
) -> HttpResponse {
    let Some(body) = req.json_body() else {
        return HttpResponse::error(400, "invalid JSON body");
    };
    let Some(addr) = body.get("address").and_then(json_to_u32) else {
        return HttpResponse::error(400, "missing 'address'");
    };
    let removed = emu.remove_breakpoint(addr);
    HttpResponse::ok(&json!({
        "address": format!("0x{:06X}", addr),
        "removed": removed,
        "breakpoints": emu.breakpoints().iter().map(|a| format!("0x{:06X}", a)).collect::<Vec<_>>(),
    }))
}

fn handle_input(req: &HttpRequest, emu: &mut dyn crate::api::EmulatorAccess) -> HttpResponse {
    let Some(body) = req.json_body() else {
        return HttpResponse::error(400, "invalid JSON body");
    };
    match emu.set_input(&body) {
        Ok(applied) => HttpResponse::ok(&json!({ "input": applied })),
        Err(e) => HttpResponse::error(400, &e),
    }
}

fn handle_framebuffer(emu: &mut dyn crate::api::EmulatorAccess) -> HttpResponse {
    let fb = emu.framebuffer();
    let w = emu.framebuffer_width();
    let h = emu.framebuffer_height();
    HttpResponse::ok(&json!({
        "width": w,
        "height": h,
        "format": "rgb565",
        "pixels": fb,
    }))
}

fn handle_save_state(req: &HttpRequest, emu: &mut dyn crate::api::EmulatorAccess) -> HttpResponse {
    let path = req
        .query_param("path")
        .or_else(|| {
            req.json_body()
                .and_then(|b| b.get("path").and_then(|v| v.as_str()).map(String::from))
        })
        .unwrap_or_else(|| "genteel_api_savestate.json".to_string());
    match emu.save_state(&path) {
        Ok(p) => HttpResponse::ok(&json!({ "saved": p })),
        Err(e) => HttpResponse::error(500, &e),
    }
}

fn handle_load_state(req: &HttpRequest, emu: &mut dyn crate::api::EmulatorAccess) -> HttpResponse {
    let path = req.query_param("path").or_else(|| {
        req.json_body()
            .and_then(|b| b.get("path").and_then(|v| v.as_str()).map(String::from))
    });
    let Some(path) = path else {
        return HttpResponse::error(400, "missing 'path'");
    };
    match emu.load_state(&path) {
        Ok(()) => HttpResponse::ok(&json!({ "loaded": path, "state": emu.state_json() })),
        Err(e) => HttpResponse::error(500, &e),
    }
}

/// Render bytes as a continuous lowercase hex string.
fn hex_string(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Render bytes as ASCII (non-printables become '.').
fn ascii_string(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| {
            if (0x20..=0x7E).contains(&b) {
                b as char
            } else {
                '.'
            }
        })
        .collect()
}

/// Parse a hex string (optionally `0x`-prefixed, whitespace ignored) to bytes.
fn parse_hex(s: &str) -> Option<Vec<u8>> {
    let cleaned: String = s
        .trim()
        .trim_start_matches("0x")
        .trim_start_matches("0X")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if !cleaned.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(cleaned.len() / 2);
    let bytes = cleaned.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let hi = (bytes[i] as char).to_digit(16)?;
        let lo = (bytes[i + 1] as char).to_digit(16)?;
        out.push(((hi << 4) | lo) as u8);
        i += 2;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_addr_accepts_hex_and_decimal() {
        assert_eq!(parse_addr("0xFF0000"), Some(0xFF0000));
        assert_eq!(parse_addr("$C00000"), Some(0xC00000));
        assert_eq!(parse_addr("255"), Some(255));
        assert_eq!(parse_addr("nope"), None);
    }

    #[test]
    fn hex_roundtrip() {
        let bytes = [0x00u8, 0xDE, 0xAD, 0xBE, 0xEF];
        assert_eq!(hex_string(&bytes), "00deadbeef");
        assert_eq!(parse_hex("00deadbeef"), Some(bytes.to_vec()));
        assert_eq!(parse_hex("0xDEAD"), Some(vec![0xDE, 0xAD]));
        assert_eq!(parse_hex("abc"), None); // odd length
    }

    #[test]
    fn ascii_renders_nonprintables() {
        let bytes = [0x41u8, 0x00, 0x7E, 0xFF];
        assert_eq!(ascii_string(&bytes), "A.~.");
    }
}
