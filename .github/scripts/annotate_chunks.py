"""Emit a log file's content as a sequence of small annotations.

Usage: annotate_chunks.py <log> <tag> [lines_per_chunk]

The log is split into chunks of `lines_per_chunk` lines (default 6); each
chunk is emitted as one ::error annotation whose title carries the chunk index
so the reader can reassemble the file in order.
"""
import subprocess
import sys

path = sys.argv[1]
tag = sys.argv[2]
size = int(sys.argv[3]) if len(sys.argv) > 3 else 6
lines = open(path, encoding="utf-8", errors="replace").read().splitlines() or ["<empty>"]
total = (len(lines) + size - 1) // size
for i in range(total):
    chunk = lines[i * size : (i + 1) * size]
    text = "%0A".join(line.replace("%", "%25").replace("\r", "")[:700] for line in chunk)
    subprocess.run(["echo", f"::error file={tag},line={i+1},title={tag}-{i:03d}::{text or '<blank>'}"])
print(f"{tag}: {len(lines)} lines in {total} chunks")
