#!/usr/bin/env python3
"""QEMU-only workaround: make every illegal-instruction trap on the P4 count as first FPU use.

ESP-IDF enables the P4 FPU lazily: a task starts with mstatus.FS off, and the illegal-instruction
handler (`_panic_handler` in riscv/vectors.S) reads the EXT_ILL CSR (0x7F0) to see whether an FPU
instruction caused the trap. Espressif QEMU (esp-develop-9.2.2-20261109) sets that reason only for
FP loads/stores and FP arithmetic, not for `csrr` of fflags/frm/fcsr. The ROM's soft-double
routines read `frm`, so the first f64 operation in a task panics with "Illegal instruction"
(MTVAL 0x00202973, `csrr s2, frm`). Silicon v3+ is assumed to report those CSR
accesses as FPU use (IDF's rev < 3 software decode counts them); not observed on hardware.

The patch replaces `csrrw a0, 0x7f0, zero` with `li a0, 1` (reason = FPU) in the handler. A real
illegal instruction then loops (FPU enabled, retried, trapped again) instead of panicking; the
QEMU scripts' timeout catches that. Only for the emulator image.

usage: qemu-patch-p4-fpu.py <nm-tool> <in.elf> <out.elf>   (in and out may be the same file)
"""
import struct
import subprocess
import sys

CSRRW_A0_EXT_ILL = struct.pack("<I", 0x7F001573)  # csrrw a0, 0x7f0, zero
LI_A0_1 = struct.pack("<I", 0x00100513)  # addi a0, zero, 1
SEARCH = 0x200  # bytes after _panic_handler that may hold the CSR read

nm, src, dst = sys.argv[1:4]
syms = subprocess.run([nm, src], capture_output=True, text=True, check=True).stdout.splitlines()
addr = next((int(s.split()[0], 16) for s in syms if s.endswith(" _panic_handler")), None)
if addr is None:
    sys.exit("_panic_handler not found")
data = bytearray(open(src, "rb").read())
assert data[:4] == b"\x7fELF" and data[4] == 1, "expected ELF32"
phoff, = struct.unpack_from("<I", data, 0x1C)
phentsize, phnum = struct.unpack_from("<HH", data, 0x2A)
for i in range(phnum):
    p_type, p_off, p_vaddr, _, p_filesz = struct.unpack_from("<IIIII", data, phoff + i * phentsize)
    if p_type == 1 and p_vaddr <= addr < p_vaddr + p_filesz:
        start = p_off + (addr - p_vaddr)
        off = data.find(CSRRW_A0_EXT_ILL, start, start + SEARCH)
        if off < 0:
            sys.exit("EXT_ILL CSR read not found after _panic_handler")
        data[off:off + 4] = LI_A0_1
        print(f"patched EXT_ILL read at {p_vaddr + off - p_off:#x} (file offset {off:#x})")
        break
else:
    sys.exit("symbol address not in a LOAD segment")
open(dst, "wb").write(data)
