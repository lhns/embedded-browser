#!/usr/bin/env python3
"""QEMU-only workaround: neutralise esp_adc's `adc_hw_calibration` constructor in an ELF.

QEMU's ESP32-S3 does not model the SAR ADC, so the constructor spins forever and the image never
reaches app_main (known issue, also hit by other projects). esp-idf-sys links all IDF components,
so the constructor is always present. The function is made to return immediately:
- Xtensa (S3): keeps the 3-byte `entry` and replaces the next 2 bytes with `retw.n`;
- RISC-V (P4): replaces the first 2 bytes with `c.ret` (the P4 target has the C extension).
The patched ELF is only used for the emulator image.

usage: qemu-patch-adc.py <nm-tool> <in.elf> <out.elf>
"""
import struct
import subprocess
import sys

EM_XTENSA, EM_RISCV = 94, 243

nm, src, dst = sys.argv[1:4]
syms = subprocess.run([nm, src], capture_output=True, text=True, check=True).stdout.splitlines()
addr = next((int(s.split()[0], 16) for s in syms if s.endswith(" adc_hw_calibration")), None)
data = bytearray(open(src, "rb").read())
if addr is None:
    print("adc_hw_calibration not linked, nothing to patch")
else:
    assert data[:4] == b"\x7fELF" and data[4] == 1, "expected ELF32"
    machine, = struct.unpack_from("<H", data, 0x12)
    phoff, = struct.unpack_from("<I", data, 0x1C)
    phentsize, phnum = struct.unpack_from("<HH", data, 0x2A)
    for i in range(phnum):
        p_type, p_off, p_vaddr, _, p_filesz = struct.unpack_from("<IIIII", data, phoff + i * phentsize)
        if p_type == 1 and p_vaddr <= addr < p_vaddr + p_filesz:
            off = p_off + (addr - p_vaddr)
            if machine == EM_XTENSA:
                assert data[off] == 0x36, "expected an entry instruction"
                data[off + 3:off + 5] = b"\x1d\xf0"  # retw.n
            elif machine == EM_RISCV:
                data[off:off + 2] = b"\x82\x80"  # c.ret
            else:
                sys.exit(f"unsupported ELF machine {machine}")
            print(f"patched adc_hw_calibration at {addr:#x} (file offset {off:#x})")
            break
    else:
        sys.exit("symbol address not in a LOAD segment")
open(dst, "wb").write(data)
