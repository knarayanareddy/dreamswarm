"""Emit a slice of a log file as GitHub check-run annotations."""
import subprocess
import sys

path, start, count = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
lines = open(path, encoding="utf-8", errors="replace").read().splitlines()
for offset, line in enumerate(lines[start:start + count], start=start + 1):
    text = line.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")[:900]
    print(f"::error file=check.log,line={offset},title=L{offset}::{text or '<blank>'}")
