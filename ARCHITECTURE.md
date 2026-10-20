# Architecture Overview

This document serves as a critical, living template designed to equip agents with a rapid and comprehensive understanding of the codebase's architecture, enabling efficient navigation and effective contribution from day one. Update this document as the codebase evolves.

## 1. Project Structure

This section provides a high-level overview of the project's directory and file structure, categorised by architectural layer or major functional area. It is essential for quickly navigating the codebase, locating relevant files, and understanding the overall organization and separation of concerns.

```text
genteel/
├── src/
│   ├── main.rs           # Application entry point
│   ├── lib.rs            # Library exports
│   ├── cpu/              # M68k CPU implementation
│   │   └── mod.rs
│   ├── apu/              # Audio Processing Unit (Z80, YM2612, SN76489)
│   │   └── mod.rs
│   ├── vdp/              # Video Display Processor
│   │   └── mod.rs
│   ├── memory/           # Memory bus and mapping
│   │   └── mod.rs
│   ├── io/               # Input/Output (controllers)
│   │   └── mod.rs
│   ├── debugger/         # GDB RSP debugging interface
│   │   └── mod.rs
│   └── api/              # Agent-facing JSON/HTTP API (mod.rs, handlers.rs, tests.rs)
│       └── ...
├── clients/              # Example API clients (python/)
│   └── python/
├── docs/                 # Additional documentation (incl. api.md)
├── scripts/              # Automation and auditing scripts
├── Cargo.toml            # Rust project manifest
├── README.md             # Project overview for humans
├── AGENTS.md             # AI agent operational context
└── ARCHITECTURE.md       # This document
```

## 2. High-Level System Diagram

Provide a simple block diagram or a clear text-based description of the major components and their interactions.

```text
[External Tool (e.g., Agent, GDB)]
        ^
        |  (1) GDB Remote Serial Protocol   (2) JSON/HTTP
        v
[Genteel Emulator (main.rs / lib.rs)] <--> [SharedBus / MemoryInterface]
        ^        ^                              |
        |        |                              +--> [CPU (M68k)]
        |        +-- src/api  (JSON/HTTP server)  +--> [APU (Z80, YM2612, PSG)]
        +-- src/debugger (GDB RSP server)         +--> [VDP (Graphics)]
                                                  +--> [I/O (Controllers)]
                                                  +--> [Memory (ROM, WRAM, VRAM, SRAM)]
```

Both external interfaces are **single-threaded** and interleaved with
emulation by the run loop, because the emulator state (`Rc<RefCell<Bus>>`) is
`!Send`. The JSON/HTTP API ([`crate::api`]) reuses the emulator's existing
disassembler, memory bus, and serde-serializable state; see `docs/api.md`.

## 3. Core Components

### 3.1. Emulator Core
Name: Emulator / System Loop
Description: Coordinates the main emulation loop, steps the M68k CPU, steps the Z80 co-processor, advances the bus/VDP/APU timing, checks for H/V interrupts, and manages bus contention. The outer loop no longer pre-renders scanlines; completed active scanlines are rendered from the VDP timing path at line end.
Technologies: Rust

### 3.2. M68k CPU
Name: CPU (`src/cpu/`)
Description: Implements the main processor, a Motorola 68000 (M68k). Responsible for fetching, decoding, and executing instructions from the game ROM and RAM.
Technologies: Rust

### 3.3. Audio Processing Unit (APU)
Name: APU (`src/apu/`)
Description: Contains the implementation of the Zilog Z80 sound co-processor, the Yamaha YM2612 FM synthesizer, and the Texas Instruments SN76489 PSG. The APU timing is configurable by video region and output sample rate, with both PSG and YM2612 clocking derived from the active Genesis master clock through the shared `Apu::set_timing` path. That timing reconfiguration now preserves final low-pass filter history when the requested timing is unchanged, instead of resetting the mixed output state on every normal sync slice, while still repairing stale child clock/sample-rate state if needed. The YM2612 core is being moved toward Genesis Plus GX behavior with native-sample timing, 42-MCLK internal-cycle stepping with per-channel subcycle clocking, sample-latched YM state so mid-sample register writes do not skew later channel slots, current-sample stereo mixing, or the already-latched current EG tick, DAC state changes that now become audible through the channel-6 sample pipeline instead of write-time blip injection, EG-tick scheduling, reference-style LFO PM tables, more accurate SSG-EG state handling, immediate active SSG-EG shape-write application, CSM keying behavior, tighter detune/phase stepping, selectable discrete-YM2612 versus YM3438 read-port behavior, single-data-port bank selection that follows the last addressed register group, profile-aware BUSY timing for address and data writes, including correct YM3438 wait-cycle conversion into the Genesis MCLK domain consumed by `step()`, data-port writes that require the port's A1 line to match the last-addressed bank and are otherwise ignored (writes are applied regardless of BUSY, matching real hardware and BlastEm; BUSY affects only the status read), deferred FNUM/block commit semantics that apply on the matching low-byte write, Timer B’s `/16` sample cadence, profile-gated DAC ladder coloration so YM3438 mode does not inherit discrete-YM2612 output offsets, profile-gated carrier quantization masks, wider internal phase-modulation buses that avoid premature 14-bit truncation in operator routing, and operator-time carrier masking so algorithm-7 feedback history sees quantized output like the reference core. Final APU mixing to the host is now linear/saturated rather than running through an artificial soft-clipping stage, the FM/PSG blend is normalized instead of boosted, the mixed output now runs through a Genesis-style first-order Butterworth low-pass filter before host delivery, the OS callback holds the last delivered sample on buffer lock misses and stereo underruns (decaying it toward zero so a stopped producer cannot park a DC level) instead of snapping to zero, queue trimming stays stereo-aligned instead of severing left/right pairs, freshly generated temp-buffer samples are not dropped just because the per-frame queue hit a cap, GUI-side audio uploads are staged locally and flushed with non-blocking locks, and emulation/audio production now runs from the event-loop pacing path with a small bounded catch-up burst before redraw so rendering stalls do not directly starve the callback while the GUI still follows the active NTSC/PAL frame cadence. The Z80 implementation handles architectural nuances like MEMPTR (WZ Register), R Register wrapping, and EI interrupt shadowing.
Technologies: Rust

### 3.4. Video Display Processor (VDP)
Name: VDP (`src/vdp/`)
Description: Responsible for rendering the graphics. It manages video RAM (VRAM), sprites, backgrounds, and generates the video output. Scanline completion is timing-driven inside `Vdp::tick`, and the renderer uses an internal Sprite Attribute Table mirror to evaluate sprites instead of treating VRAM as the sole source of visible sprite state.
Technologies: Rust

### 3.5. Memory & Bus
Name: Memory (`src/memory/`)
Description: Implements the memory bus and mapping via the `MemoryInterface` trait. The `SharedBus` wrapper allows components to share the `Bus` state. Supports full state serialization/deserialization via `serde`.
Technologies: Rust (Trait Objects, Interior Mutability, Serde)

### 3.6. I/O and Debugger
Name: I/O (`src/io/`) & Debugger (`src/debugger/`)
Description: Handles all input and output (game controllers), and provides a GDB Remote Serial Protocol (RSP) interface to allow external debuggers to connect to the emulator.
Technologies: Rust, GDB RSP

## 4. Data Stores

### 4.1. Game ROMs
Name: Primary Game ROM
Type: File (binary/zip)
Purpose: Read-Only Memory where the game's code and data are stored. Loaded into the emulator's memory at the start.

### 4.2. Internal Memory
Name: System RAM
Type: In-memory buffers
Purpose: The emulator manages several internal memory regions: Work RAM (64KB for M68k), Video RAM (64KB for VDP), and Sound RAM (8KB for Z80).

### 4.3. Save States
Name: Save Games
Type: File (binary/JSON)
Purpose: Future support for saving and loading game states, utilizing `serde` serialization to store component states.

## 5. External Integrations / APIs

Service Name 1: AI Agent API
Purpose: High-level API for AI agents to control the emulator (load ROMs, run frames, set controller state, read/write memory).
Integration Method: Rust API / Command-line arguments (`--script`, `--headless`)

Service Name 2: GDB Interface
Purpose: Allows external standard debuggers (like GDB) to connect and debug M68k code running inside the emulator.
Integration Method: Local network socket (GDB RSP)

## 6. Deployment & Infrastructure

Cloud Provider: Local / Standalone Application (Linux, macOS, Windows)
Key Services Used: Native OS execution
CI/CD Pipeline: To be configured (Supports headless validation for CI)
Monitoring & Logging: `log` and `env_logger` crates for structured output (`RUST_LOG=debug`)

## 7. Security Considerations

Authentication: N/A
Authorization: N/A
Data Encryption: N/A
Key Security Tools/Practices:
- Bounds-check all memory accesses (emulator runs untrusted ROM code).
- Handle invalid/malformed instructions gracefully.
- The debugger interface uses local network sockets - restrict access appropriately.
- Run the audit tool (`python3 scripts/audit_tool.py`) periodically to detect secrets and unsafe patterns.

## 8. Development & Testing Environment

Local Setup Instructions: `cargo build` (debug) or `cargo build --release`
Testing Frameworks: `cargo test` (unit/integration), `proptest` (property-based tests), `cargo-fuzz` (fuzzing)
Code Quality Tools: `cargo clippy`, `cargo fmt`, `make audit` (custom audit script)

## 9. Future Considerations / Roadmap

- **M68k Implementation**: Completed full instruction set and addressing modes; fixed bugs in ADDX/SUBX/EXG.
- **APU Implementation**: Initial Yamaha YM2612 FM and SN76489 PSG support implemented; fixed test regressions.
- **AI Agent API**: JSON/HTTP API (`src/api/`) exposing state, registers, memory
  read/write, single-step/frame stepping, M68k disassembly (Motorola + GNU
  syntax), breakpoints, controller input injection, framebuffer capture, and
  savestate save/load. Enabled with `--api [port]`. See `docs/api.md` and
  `clients/python/`.
- **Controller Support**: 3-button and 6-button controller support implemented.
- **GDB Support**: Basic RSP support with breakpoints and inspection implemented.
- **Accuracy Improvements**: Moved VBlank/HBlank/LineCounter management into VDP `tick` for better cycle accuracy.
- **Accuracy Improvements**: Rendering ownership is now centered in `Vdp::tick` for completed scanlines, with SAT mirroring for sprite evaluation and region-aware APU timing configuration.
- **32X Expansion**: Future goal (dual SH2 cores, Master/Slave sync, 32X VDP).

## 10. Project Identification

Project Name: genteel
Repository URL: https://github.com/segin/genteel
Primary Contact/Team: N/A
Date of Last Update: 2026-10-12 (added the agent-facing JSON/HTTP API in `src/api/`: state/registers/memory/disassemble/step/run/pause/reset/breakpoints/input/framebuffer/savestate endpoints, a dependency-free `std::net` HTTP server, an `EmulatorAccess` bridge in `main.rs`, `--api`/`--api-password`/`--api-bind` CLI flags, a Python client under `clients/python/`, and `docs/api.md`. Earlier: 2026-07-04 audit fixes: GPGX-style envelope increment table, register-order slot offsets, algorithm 4/5/6 routing corrections, Timer B period without the doubled /16 prescaler, A1-bank-matched data-port writes replacing BUSY-gated write rejection, serde defaults for save-state compatibility, frame-pacer stall resync, audio callback 0-channel guard and DC decay. Earlier YM2612 parity work: added discrete-vs-YM3438 read-port behavior, single-data-port bank selection, profile-aware BUSY timing, corrected YM3438 BUSY wait-cycle units, deferred FNUM/block commit semantics, 42-MCLK internal-cycle YM stepping with per-channel subcycle clocking, sample-latched YM state for current-sample output and EG-tick coherence, DAC writes routed through the channel-6 sample pipeline instead of write-time blip injection, Timer B /16 cadence, profile-gated ladder coloration, profile-gated carrier quantization, wider internal phase-modulation buses, operator-time carrier masking, immediate active SSG-EG shape-write application, Genesis-style first-order Butterworth low-pass filtering on final APU output, linear final APU mixing, normalized FM/PSG blending, callback-side last-sample hold on both lock misses and stereo underruns, preserved APU filter state across unchanged timing syncs, stereo-aligned queue trimming, removal of temp-buffer sample dropping, staged non-blocking GUI audio uploads, moving emulation/audio production into the event-loop pacing path, bounded frame catch-up before redraw, PAL-aware GUI frame pacing, reference-style PM tables, and tighter SSG-EG/CSM/detune handling alongside prior region-aware APU timing updates)

## 11. Glossary / Acronyms

M68k: Motorola 68000, the main CPU of the Sega Mega Drive/Genesis
VDP: Video Display Processor, the custom graphics chip
Z80: Zilog Z80, an 8-bit CPU used as a sound co-processor
YM2612: A six-voice FM synthesis sound chip
SN76489: A programmable sound generator (PSG) chip
ROM: Read-Only Memory, where the game's code and data are stored
GDB RSP: The GDB Remote Serial Protocol, a protocol for remote debugging
VRAM: Video RAM, memory used by the VDP for graphics
WRAM: Work RAM, general-purpose memory for the M68k
