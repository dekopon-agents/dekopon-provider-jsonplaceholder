#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$root"

python3 - .github/workflows/*.yml <<'PY'
import pathlib
import re
import sys

sha = re.compile(r"^[0-9a-f]{40}(?:\s+#.*)?$")
for name in sys.argv[1:]:
    path = pathlib.Path(name)
    text = path.read_text(encoding="utf-8")
    if "permissions:" not in text:
        raise SystemExit(f"{path}: workflow must declare permissions")
    for number, line in enumerate(text.splitlines(), 1):
        stripped = line.strip()
        if not stripped.startswith("uses:"):
            continue
        value = stripped.removeprefix("uses:").strip()
        if value.startswith("./"):
            continue
        if "@" not in value:
            raise SystemExit(f"{path}:{number}: Action reference has no revision")
        revision = value.rsplit("@", 1)[1]
        if not sha.fullmatch(revision):
            raise SystemExit(f"{path}:{number}: Action is not pinned to a full commit SHA")
PY

[[ "$(actionlint -version | head -1)" == *"1.7.12"* ]] || {
  echo "error: actionlint 1.7.12 is required" >&2
  exit 1
}
actionlint -color
printf 'workflow YAML and full-SHA Action references passed actionlint\n'
