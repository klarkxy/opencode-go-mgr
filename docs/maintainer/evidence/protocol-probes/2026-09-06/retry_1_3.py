import json
import time
from pathlib import Path

from probe_zen_free import probe

rows = []
for stream in (False, True):
    row = probe("muse-spark-1.3-contributor-free", "responses", stream)
    rows.append(row)
    print(
        f"retry 1.3 responses stream={int(stream)} status={row['status']} ok={row['ok']} {row['durationMs']}ms"
    )
    print(" ", row["evidence"][:240])
    time.sleep(2)

out = Path(__file__).with_name("retry-1.3-responses.jsonl")
out.write_text("\n".join(json.dumps(row, ensure_ascii=False) for row in rows) + "\n", encoding="utf-8")
print("wrote", out)
