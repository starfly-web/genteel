# AGENTS.md

This document provides AI-specific operational context for working on the `genteel` project. For general project information, see [README.md](README.md). For architecture details, see [ARCHITECTURE.md](ARCHITECTURE.md).

## Quick Reference

- **Language**: Rust
- **Build**: `cargo build` (debug) or `cargo build --release` (release)
- **Generate Builds**: `python3 scripts/generate_builds.py` (Linux + Windows)
- **Test**: `cargo test`
- **Run**: `cargo run`
- **Check**: `cargo clippy`
- **Format**: `cargo fmt`
- **Audit**: `python3 scripts/audit_tool.py`

## Logging and Debugging

The project uses the `log` crate for structured output.
- **Default**: Errors and warnings are shown.
- **Verbose**: Use `RUST_LOG=debug cargo run -- --debug` to see detailed execution traces.
- **Release**: Most high-frequency logs are disabled in release builds to maintain performance.

### In-Game Debugging
When running with the GUI enabled, a **Performance & Debug** window is available. It shows:
- Frontend FPS and frame times
- Internal emulation frame count
- Real-time Program Counter (PC) for both M68k and Z80
- VDP Status and Display state

## Agent Operating Principles

### 1. Keep ARCHITECTURE.md in Sync

The `ARCHITECTURE.md` file is the **source of truth** for the project's design. When making architectural changes:

1. **Read ARCHITECTURE.md first** before making significant changes
2. **Update ARCHITECTURE.md** whenever you modify the project's structure or design
3. **Update the "Date of Last Update" field** when modifying ARCHITECTURE.md

### 2. Commit and Push on Task Completion

At the end of each successfully completed task:

1. Commit all changes with a clear, descriptive commit message
2. Immediately push to the remote repository (`git push`)

### 3. Aggressive Testing

All new features, especially CPU opcodes, must be accompanied by comprehensive tests:

- **Unit tests**: Standard operation and edge cases
- **Property-based tests**: Use `proptest` crate for wide input coverage
- **Flag coverage**: 
  - M68k: C, V, Z, N, X flags
  - Z80: S, Z, Y, H, X, P/V, N, C flags (must include undocumented X/Y)

### 4. Z80 Architectural Integrity

Agents must implement and verify the following Z80 nuances for high compatibility:

- **MEMPTR (WZ)**: Must be updated during 16-bit loads, block ops, and bit tests.
- **Undocumented Flags (X/Y)**: Must leak from MEMPTR or Effective Address during `BIT` operations.
- **R Register**: Bit 7 must be stable during fetch; only lower 7 bits increment.
- **Interrupts**: `EI` instructions must have a 1-instruction shadow. `IM 2` must use a bus-provided vector.

### 5. Memory & Bus Integration

The emulator uses a unified `MemoryInterface` trait for all bus interactions.

- **Mutable Reads**: The `read` methods require `&mut self` because some hardware components (like VDP or certain I/O registers) have side-effects on read.
- **Shared Access**: Use `SharedBus` (which wraps `Rc<RefCell<Bus>>`) for components that need to share the main Genesis bus.
- **Concrete Memory**: For isolated RAM (like Z80 Sound RAM), prefer concrete `Memory` struct unless generic bus participation is required.

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_name
```

## Code Style

- Use `cargo fmt` before committing
- Use `cargo clippy` to check for common issues
- Prefer functional patterns where possible
- Document public APIs with rustdoc comments
- Use block comments `/* ... */` for multi-line comments.
- Always reference Pull Requests by their number (e.g., PR #123), never by branch name.
- Keep functions small and focused

## Security Considerations

- Bounds-check all memory accesses (emulator runs untrusted ROM code)
- Handle invalid/malformed instructions gracefully
- The debugger interface uses local network sockets - restrict access appropriately
- Run the audit tool (`python3 scripts/audit_tool.py`) periodically to detect secrets and unsafe patterns.

## Agent Capabilities

AI agents working on this project can:

- **Develop the emulator**: Implement features from the roadmap in ARCHITECTURE.md
- **Play games**: Send controller inputs to test game behavior
- **Perform targeted testing**: Test specific functionality
- **Fuzzing**: Generate random inputs to test robustness
- **Analyze game states**: Read emulator memory to analyze internal state
- **Drive the running emulator over HTTP**: Launch with `--api [port]` and use
  the JSON/HTTP API to read/write registers & memory, single-step or run frames,
  disassemble M68k code, manage breakpoints, inject input, capture the
  framebuffer, and save/load state. See `docs/api.md` and the ready-to-use
  client in `clients/python/genteel_client.py`.

---

*This file follows the [AGENTS.md](https://agents.md/) specification.*
