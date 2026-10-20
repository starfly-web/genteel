#!/usr/bin/env python3
"""Example: drive Genteel's agent API to reverse-engineer a routine.

This is a template an AI agent can adapt. It demonstrates the typical
observe -> act -> observe loop: read state, single-step, disassemble, poke
memory, and trace a subroutine.

Prerequisites:
    * Build:  cargo build --no-default-features
    * Run:    ./target/debug/genteel --api 4545 path/to/rom.md
    * Then:   python3 clients/python/example_re_scan.py

The script makes no assumptions about a specific ROM; it just exercises the
API against whatever is currently loaded.
"""

from genteel_client import Genteel, GenteelError


def main() -> None:
    g = Genteel()

    print("== info ==")
    info = g.info()
    print("  system:", info.get("system"), "| api_version:", info.get("api_version"))
    print("  rom:   ", info.get("rom"))

    print("\n== initial state ==")
    st = g.state()
    regs = st.get("registers", {})
    print("  PC:", regs.get("pc"), " SR:", regs.get("sr"), " frame:", st.get("frame"))

    # 1) Disassemble a window around the current PC.
    pc = int(regs.get("pc", "0x0"), 16)
    print(f"\n== disassembly @ PC ({hex(pc)}) ==")
    for line in g.disassemble(pc, count=10):
        print("  ", line)

    # 2) Single-step 5 instructions and watch the PC move.
    print("\n== single-step x5 ==")
    for _ in range(5):
        g.step(kind="instruction", count=1)
        pc_now = g.state()["registers"]["pc"]
        print("  PC ->", pc_now)

    # 3) Read a chunk of work RAM (0xFF0000..).
    print("\n== work RAM @ 0xFF0000 (32 bytes) ==")
    mem = g.read_memory(0xFF0000, 32)
    print("  hex:   ", mem.get("hex"))
    print("  ascii: ", mem.get("ascii"))

    # 4) Poke a register and memory, then verify.
    print("\n== write register d0 = 0x00001234 ==")
    g.write_register("d0", 0x1234)
    print("  d0 ->", g.registers().get("d0"))

    print("\n== write memory 0xFF0100 = DE AD BE EF ==")
    g.write_memory(0xFF0100, "deadbeef")
    print("  readback:", g.read_memory(0xFF0100, 4).get("hex"))

    # 5) Set a breakpoint at the current PC, list, then clear it.
    pc_now = int(g.state()["registers"]["pc"], 16)
    print(f"\n== breakpoints ==")
    g.add_breakpoint(pc_now)
    print("  set:", g.breakpoints())
    g.remove_breakpoint(pc_now)
    print("  after remove:", g.breakpoints())

    # 6) Run one frame and report the new frame counter. The API starts
    #    paused for determinism, so resume first, step a frame, then pause.
    print("\n== run 1 frame ==")
    g.run()
    before = g.state()["frame"]
    g.step(kind="frame", count=1)
    after = g.state()["frame"]
    g.pause()
    print(f"  frame {before} -> {after}")

    print("\nDone. Adapt this loop for your reverse-engineering task.")


if __name__ == "__main__":
    try:
        main()
    except GenteelError as e:
        print("API error:", e)
        raise SystemExit(1)
