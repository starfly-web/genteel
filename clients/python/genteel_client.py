#!/usr/bin/env python3
"""Dependency-free Python client for the Genteel agent API.

Designed so AI agents (Cline, Kilo, etc.) can drive the emulator for reverse
engineering with plain HTTP + JSON. Only the standard library is used.

Example
-------
    from genteel_client import Genteel

    g = Genteel()                       # http://127.0.0.1:4545
    print(g.state())                    # registers, frame, region, ...
    print(g.disassemble(0x200, 8))      # disassemble 8 instructions at 0x200
    g.step(kind="instruction", count=5) # single-step 5 instructions
    print(g.read_memory(0xFF0000, 16))  # hex dump of work RAM
    g.write_register("d0", 0x1234)
"""

from __future__ import annotations

import json
import urllib.request
import urllib.error
from typing import Any, Dict, Iterable, List, Optional, Union


class GenteelError(Exception):
    """Raised when the API returns a non-2xx response."""


class Genteel:
    def __init__(
        self,
        host: str = "127.0.0.1",
        port: int = 4545,
        token: Optional[str] = None,
        timeout: float = 30.0,
    ) -> None:
        self.base = f"http://{host}:{port}"
        self.token = token
        self.timeout = timeout

    # -- low level -----------------------------------------------------------
    def _request(
        self,
        method: str,
        path: str,
        body: Optional[Dict[str, Any]] = None,
        params: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        url = self.base + path
        if params:
            from urllib.parse import urlencode

            qs = urlencode({k: v for k, v in params.items() if v is not None})
            if qs:
                url += ("&" if "?" in url else "?") + qs

        data = None
        headers = {"Accept": "application/json"}
        if body is not None:
            data = json.dumps(body).encode("utf-8")
            headers["Content-Type"] = "application/json"
        if self.token:
            headers["Authorization"] = f"Bearer {self.token}"

        req = urllib.request.Request(url, data=data, headers=headers, method=method)
        try:
            with urllib.request.urlopen(req, timeout=self.timeout) as resp:
                raw = resp.read()
        except urllib.error.HTTPError as e:  # non-2xx
            detail = e.read().decode("utf-8", "replace")
            try:
                detail = json.loads(detail).get("error", detail)
            except Exception:
                pass
            raise GenteelError(f"HTTP {e.code} {method} {path}: {detail}") from None

        if not raw:
            return {}
        return json.loads(raw.decode("utf-8"))

    # -- endpoints -----------------------------------------------------------
    def info(self) -> Dict[str, Any]:
        """API version + capabilities."""
        return self._request("GET", "/info")

    def state(self) -> Dict[str, Any]:
        """Full machine snapshot (registers, z80, frame, region, paused)."""
        return self._request("GET", "/state")

    def registers(self) -> Dict[str, Any]:
        """Just the CPU register block (unwrapped from the `registers` key)."""
        return self._request("GET", "/registers").get("registers", {})

    def write_register(self, name: str, value: Union[int, str]) -> Dict[str, Any]:
        """Write a single register, e.g. write_register('d0', 0x1234)."""
        return self._request("POST", "/registers", {"registers": {name: value}})

    def write_registers(self, **regs: Union[int, str]) -> Dict[str, Any]:
        """Write multiple registers, e.g. write_registers(d0=1, pc=0x200)."""
        return self._request("POST", "/registers", {"registers": regs})

    def step(self, kind: str = "instruction", count: int = 1) -> Dict[str, Any]:
        """Advance the machine. kind is 'instruction' or 'frame'."""
        return self._request("POST", "/step", {"type": kind, "count": count})

    def run(self) -> Dict[str, Any]:
        """Resume continuous execution."""
        return self._request("POST", "/run")

    def pause(self) -> Dict[str, Any]:
        """Pause continuous execution."""
        return self._request("POST", "/pause")

    def reset(self) -> Dict[str, Any]:
        """Hard reset the machine."""
        return self._request("POST", "/reset")

    def read_memory(
        self, address: int, length: int = 16, fmt: str = "both"
    ) -> Dict[str, Any]:
        """Read `length` bytes at `address`. fmt: 'hex'|'ascii'|'bytes'|'both'."""
        return self._request(
            "GET",
            "/memory",
            params={"address": hex(address), "length": length, "format": fmt},
        )

    def read_bytes(self, address: int, length: int = 16) -> List[int]:
        """Convenience: return the raw byte list for a memory read."""
        return self.read_memory(address, length, fmt="bytes").get("bytes", [])

    def write_memory(
        self, address: int, data: Union[bytes, bytearray, Iterable[int], str]
    ) -> Dict[str, Any]:
        """Write bytes at `address`. `data` may be raw bytes or a hex string."""
        if isinstance(data, str):
            body = {"address": hex(address), "hex": data}
        else:
            body = {"address": hex(address), "bytes": list(data)}
        return self._request("POST", "/memory", body)

    def disassemble(
        self, address: Optional[int] = None, count: int = 10, syntax: str = "motorola"
    ) -> List[str]:
        """Disassemble `count` instructions. Returns a list of text lines.

        If `address` is None the current PC is used.
        """
        params: Dict[str, Any] = {"count": count, "syntax": syntax}
        if address is not None:
            params["address"] = hex(address)
        result = self._request("GET", "/disassemble", params=params)
        return [
            f"{i['address']}:  {i['instruction']}"
            for i in result.get("instructions", [])
        ]

    def breakpoints(self) -> List[str]:
        """List current breakpoints as hex strings."""
        return self._request("GET", "/breakpoints").get("breakpoints", [])

    def add_breakpoint(self, address: int) -> Dict[str, Any]:
        return self._request("POST", "/breakpoints", {"address": hex(address)})

    def remove_breakpoint(self, address: int) -> Dict[str, Any]:
        return self._request("DELETE", "/breakpoints", {"address": hex(address)})

    def set_input(self, **buttons: bool) -> Dict[str, Any]:
        """Set controller 1 buttons, e.g. set_input(a=True, start=True).

        Recognized buttons: up down left right a b c start x y z mode.
        """
        return self._request("POST", "/input", {"buttons": buttons})

    def framebuffer(self) -> Dict[str, Any]:
        """Return the VDP framebuffer as RGB565 u16 values + dimensions."""
        return self._request("GET", "/framebuffer")

    def save_state(self, path: str = "genteel_api_savestate.json") -> Dict[str, Any]:
        return self._request("POST", "/savestate/save", {"path": path})

    def load_state(self, path: str) -> Dict[str, Any]:
        return self._request("POST", "/savestate/load", {"path": path})


if __name__ == "__main__":
    # Tiny smoke test when run directly: print info + state.
    g = Genteel()
    print("info:  ", json.dumps(g.info(), indent=2))
    print("state: ", json.dumps(g.state(), indent=2))
    print("disasm @ PC:")
    for line in g.disassemble(count=5):
        print("  ", line)
