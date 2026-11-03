"""Emit a slice of a log file as GitHub annotations.

Usage: annotate.py <log> <start> <count>
Prints check-run commands; the workflow's `run` supplies the args.
"""
import subprocess
import sys

path, start, count = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
lines = open(path, encoding="utf-8", errors="replace").read().splitlines()
for offset in range(start, min(start + count, len(lines))):
    text = lines[offset].replace("%", "%25").replace("\r", "%0D")[:800]
    subprocess.run(["echo", f"::error file=check.log,line={offset+1},title=L{offset+1}::{text or '<blank>'}"])
