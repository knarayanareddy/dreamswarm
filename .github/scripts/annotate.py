"""Emit a 2-line slice of a log file as GitHub annotations."""
import subprocess
import sys

path, start = sys.argv[1], int(sys.argv[2])
lines = open(path, encoding="utf-8", errors="replace").read().splitlines()
for offset in range(start, min(start + 2, len(lines))):
    text = lines[offset].replace("%", "%25").replace("\r", "%0D")[:800]
    subprocess.run(["echo", f"::error file=check.log,line={offset+1},title=L{offset+1}::{text or '<blank>'}"])
