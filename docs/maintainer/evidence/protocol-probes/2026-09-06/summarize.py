import json
from collections import defaultdict
from pathlib import Path

p = Path(__file__).with_name("all-attempts.jsonl")
rows = [json.loads(line) for line in p.read_text(encoding="utf-8").splitlines() if line.strip()]
grouped = defaultdict(list)
for row in rows:
    grouped[(row["modelId"], row["protocol"])].append(row)
for (model, proto), items in grouped.items():
    items = sorted(items, key=lambda row: row["stream"])
    print("=" * 80)
    print(model, proto)
    for row in items:
        print(
            f"  stream={int(row['stream'])} status={row['status']} ok={row['ok']} {row['durationMs']}ms"
        )
        print("   ", row["evidence"][:280])
