"""Minimal GDB Remote Serial Protocol client for QEMU's gdbstub.

Just enough to drive fault-injection experiments against the emulated
ESP32: software breakpoints, memory read/write, continue/interrupt. No
gdb binary, no pygdbmi — the harness stays dependency-free.

QEMU quirks relied on here (qemu-system-xtensa, espressif fork):
  - breakpoints are cpu_breakpoint based (not memory-patching), so they
    survive guest resets and flash reloads;
  - a stop reply is sent when a breakpoint hits; `c` resumes.

Usage:
    with Rsp("localhost", 3333) as r:
        r.set_bp(0x4015d6d0)
        r.cont_until_stop()          # runs until the breakpoint hits
        r.write_mem(0x3ffb2410, b"\xaa" * 0x84)
        r.clear_bp(0x4015d6d0)
        r.cont_nowait()              # fire and forget
"""

from __future__ import annotations

import socket


class RspError(Exception):
    pass


class Rsp:
    def __init__(self, host: str, port: int, timeout: float = 20.0):
        self.sock = socket.create_connection((host, port), timeout=timeout)
        self.sock.settimeout(timeout)
        self._buf = b""

    def __enter__(self) -> "Rsp":
        return self

    def __exit__(self, *exc) -> None:
        self.close()

    def close(self) -> None:
        try:
            self.sock.close()
        except OSError:
            pass

    # -- framing ----------------------------------------------------------

    @staticmethod
    def _csum(payload: bytes) -> bytes:
        return b"%02x" % (sum(payload) & 0xFF)

    def _send(self, payload: bytes) -> None:
        self.sock.sendall(b"$" + payload + b"#" + self._csum(payload))

    def _recv_byte(self) -> bytes:
        if not self._buf:
            data = self.sock.recv(4096)
            if not data:
                raise RspError("gdbstub closed the connection")
            self._buf = data
        b, self._buf = self._buf[:1], self._buf[1:]
        return b

    def _recv_packet(self) -> bytes:
        # skip acks/nacks until a packet start
        while True:
            b = self._recv_byte()
            if b == b"$":
                break
        payload = b""
        while True:
            b = self._recv_byte()
            if b == b"#":
                break
            payload += b
        csum = self._recv_byte() + self._recv_byte()
        if csum.lower() != self._csum(payload):
            raise RspError(f"bad checksum on {payload!r}")
        self.sock.sendall(b"+")  # ack
        return payload

    def _cmd(self, payload: bytes) -> bytes:
        self._send(payload)
        return self._recv_packet()

    # -- operations -------------------------------------------------------

    def set_bp(self, addr: int) -> None:
        r = self._cmd(b"Z0,%x,2" % addr)
        if r != b"OK":
            raise RspError(f"set_bp({addr:#x}) -> {r!r}")

    def clear_bp(self, addr: int) -> None:
        r = self._cmd(b"z0,%x,2" % addr)
        if r != b"OK":
            raise RspError(f"clear_bp({addr:#x}) -> {r!r}")

    def read_mem(self, addr: int, length: int) -> bytes:
        r = self._cmd(b"m%x,%x" % (addr, length))
        if r.startswith(b"E"):
            raise RspError(f"read_mem({addr:#x},{length}) -> {r!r}")
        return bytes.fromhex(r.decode())

    def write_mem(self, addr: int, data: bytes) -> None:
        r = self._cmd(b"M%x,%x:%s" % (addr, len(data), data.hex().encode()))
        if r != b"OK":
            raise RspError(f"write_mem({addr:#x}) -> {r!r}")

    def cont_until_stop(self) -> bytes:
        """Resume and block until the next stop reply (breakpoint hit)."""
        self._send(b"c")
        return self._recv_packet()  # e.g. b"T05..."

    def cont_nowait(self) -> None:
        """Resume without waiting for a stop; detaches cleanly after."""
        self._send(b"c")

    def detach(self) -> None:
        try:
            self._send(b"D")
        except OSError:
            pass
        self.close()
