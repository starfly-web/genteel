# Agent JSON/HTTP API

Genteel exposes a dependency-free JSON/HTTP API (built on `std::net`) so AI
agents and other tools can drive the emulator for reverse engineering: read and
write memory and registers, single-step or run frames, disassemble M68k code,
manage breakpoints, inject controller input, capture the framebuffer, and
save/load state.

Source: `src/api/` (`mod.rs` = HTTP server, `handlers.rs` = routing/handlers,
`tests.rs` = unit + live integration tests). A ready-to-use Python client lives
in `clients/python/genteel_client.py`, and a worked example is in
`clients/python/example_re_scan.py`.

## Running

```bash
# Build (headless — no GUI deps required)
cargo build --no-default-features

# Start the API on the default port 4545, paused for deterministic inspection
./target/debug/genteel --api 4545 path/to/rom.md

# Custom port + require a bearer token + custom bind address
./target/debug/genteel --api 8080 --api-password s3cr3t --api-bind 127.0.0.1 rom.md
```

The server binds to `127.0.0.1` by default. When a password is set, send it as
`Authorization: Bearer <token>` on every request except `GET /` and `GET /info`.

The machine **starts paused**. Call `POST /run` to let it advance between
requests; call `POST /pause` to stop. Single-stepping and inspection work while
paused and are fully deterministic.

## Conventions

- All request/response bodies are JSON (`Content-Type: application/json`).
- Addresses and register values may be given as JSON numbers **or** strings.
  String addresses accept `0x…`, `$…`, or plain decimal (`"FF0000"` →
  `0xFF0000`, `"255"` → `255`).
- Responses use standard HTTP status codes: `200` success, `400` bad request,
  `401` unauthorized, `404` unknown route, `500` server error. Error bodies are
  `{"error": "message"}`.
- The server is single-threaded and interleaved with emulation (the emulator
  state is `!Send`), so requests are serialized and never race the machine.

## Quick start (curl)

```bash
curl -s localhost:4545/info
curl -s localhost:4545/state
curl -s 'localhost:4545/disassemble?address=0x200&count=8'
curl -s -X POST localhost:4545/step -H 'Content-Type: application/json' \
     -d '{"type":"instruction","count":5}'
curl -s 'localhost:4545/memory?address=0xFF0000&length=32&format=hex'
```

## Quick start (Python)

```python
from genteel_client import Genteel
g = Genteel()                       # http://127.0.0.1:4545
print(g.state()["registers"]["pc"])
print(g.disassemble(0x200, 8))      # list of "addr:  MNEMONIC operands"
g.step(kind="instruction", count=5) # single-step 5 instructions
print(g.read_memory(0xFF0000, 16))  # hex + ascii dump
g.write_register("d0", 0x1234)
```

## Endpoint reference

| Method | Path | Description |
|---|---|---|
| GET | `/info` | API version, system name, loaded ROM, endpoint list. Never requires auth. |
| GET | `/state` | Full snapshot: D0–D7, A0–A7, USP, SSP, PC, SR, decoded CCR flags, supervisor bit, interrupt mask, Z80 PC/SP, frame counter, region, paused. |
| GET | `/registers` | Just the `registers` object from `/state`. |
| POST | `/registers` | Write registers. Body: `{"registers":{"pc":"0x200","d0":4660,…}}` or a flat object. |
| POST | `/step` | Advance the machine. Body `{"type":"instruction"\|"frame","count":N}` (count ≤ 100000). Returns new `/state`. |
| POST | `/run` | Resume continuous execution. |
| POST | `/pause` | Pause continuous execution. |
| POST | `/reset` | Hard reset (keeps the loaded ROM). |
| GET | `/memory` | Read bytes. Query: `address`, `length` (≤ 65536), `format` = `hex`\|`ascii`\|`bytes`\|`both`. |
| POST | `/memory` | Write bytes. Body `{"address":"0xFF0000","bytes":[…]}` or `{"address":…,"hex":"deadbeef"}`. |
| GET | `/disassemble` | Disassemble. Query: `address` (defaults to PC), `count` (≤ 4096), `syntax` = `motorola`\|`gnu`. |
| GET | `/breakpoints` | List breakpoints as hex strings. |
| POST | `/breakpoints` | Add. Body `{"address":"0x1234"}`. |
| DELETE | `/breakpoints` | Remove. Body `{"address":"0x1234"}`. |
| POST | `/input` | Set controller 1. Body `{"buttons":{"a":true,"start":true}}` or flat `{"a":true}`. |
| GET | `/framebuffer` | VDP framebuffer as RGB565 `u16` array + `width`/`height`. |
| POST | `/savestate/save` | Save state. Body/query `{"path":"state.json"}`. |
| POST | `/savestate/load` | Load state. Body/query `{"path":"state.json"}`. |

### Response shapes (examples)

`GET /state`:
```json
{
  "registers": {
    "d0": "0x00000000", "a7": "0x00FFF000", "pc": "0x000200",
    "sr": "0x2700",
    "ccr": {"c": false, "v": false, "z": false, "n": false, "x": false},
    "supervisor": true, "interrupt_mask": 7
  },
  "z80": {"pc": "0x0000", "sp": "0xFFFF"},
  "frame": 0, "paused": true, "region": "ntsc"
}
```

`GET /disassemble?address=0x200&count=2`:
```json
{
  "address": "0x000200", "syntax": "motorola", "count": 2,
  "instructions": [
    {"address": "0x000200", "bytes": ["7011"], "instruction": "MOVEQ   #$11,D0"},
    {"address": "0x000202", "bytes": ["7222"], "instruction": "MOVEQ   #$22,D1"}
  ]
}
```

`GET /memory?address=0xFF0000&length=4&format=hex`:
```json
{"address": "0xFF0000", "length": 4, "hex": "deadbeef"}
```

## Typical reverse-engineering loop

1. `GET /state` — read PC and registers.
2. `GET /disassemble?address=<pc>&count=N` — read the code window.
3. `POST /step {"type":"instruction","count":1}` — advance one instruction.
4. `GET /memory?address=…&length=…` — inspect data / `POST /memory` to patch.
5. `POST /breakpoints {"address":…}` — set a breakpoint, then `POST /run` to
   reach it.
6. `POST /input` + `POST /step {"type":"frame"}` — drive the game and observe.

## Security notes

- Binds to loopback (`127.0.0.1`) by default; do not expose to untrusted
  networks. Use `--api-password` to require a bearer token (compared in
  constant time via the `subtle` crate).
- Request bodies are capped at 1 MiB; memory reads/writes at 64 KiB; disassembly
  at 4096 instructions; a single step at 100000 instructions/frames.
- Savestate paths are written/read as given — keep them within trusted
  directories.

