#!/usr/bin/env python3
"""List Xtensa functions whose prologue realigns the stack pointer after `entry`.

The esp LLVM (esp toolchain 1.95, still open upstream) realigns a frame with over-aligned locals
(align > 16) by adding to a1 right after `entry` (`and aX, a1, ..; sub; add a1, a1, aX`), without
`movsp` and without a frame pointer. Stack-passed arguments are then read relative to the moved
a1, i.e. from the wrong address, and window spills can hit the caller's save area. This caused
the S3 paint crash (docs/m0/emulation-results.md). The firmware must have none of these.

usage: <xtensa objdump> -d --no-show-raw-insn firmware.elf | scan-xtensa-realign.py
Exit 1 if any function realigns.
"""
import re
import sys

PROLOGUE = 8  # instructions after the symbol that are checked
func, ops, hits = None, [], []
for line in sys.stdin:
    m = re.match(r"^[0-9a-f]+ <(.+)>:$", line)
    if m:
        func, ops = m.group(1), []
        continue
    m = re.match(r"^\s*[0-9a-f]+:\s+(\S+)\s*(.*)$", line)
    if not m or func is None or len(ops) >= PROLOGUE:
        continue
    ops.append((m.group(1), m.group(2).replace(" ", "")))
    if len(ops) == PROLOGUE and ops[0][0] == "entry":
        moves_sp = any(op.startswith("add") and args.startswith("a1,a1,") for op, args in ops)
        masks_sp = any(op == "and" and ",a1," in args + "," for op, args in ops)
        if moves_sp and masks_sp:
            hits.append(func)
for f in hits:
    print(f)
print(f"{len(hits)} realigning function(s)", file=sys.stderr)
sys.exit(1 if hits else 0)
