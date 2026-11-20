#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LIST="${EXTENSIONS_LIST:-$ROOT/docs/extensions.txt}"
OUT="${EXTENSIONS_OUTPUT:-$ROOT/docs/extensions.json}"

python3 - "$ROOT" "$LIST" "$OUT" <<'PY'
import io
import json
import sys
import urllib.parse
import urllib.request
import zipfile
from pathlib import Path

MAX_BYTES = 64 * 1024 * 1024

root = Path(sys.argv[1]).resolve()
list_path = Path(sys.argv[2])
out_path = Path(sys.argv[3])

if not list_path.exists():
    raise SystemExit(f"build-extensions: list not found: {list_path}")

sources = [
    line.strip()
    for line in list_path.read_text().splitlines()
    if line.strip() and not line.strip().startswith("#")
]

def read_source(source: str) -> bytes:
    if source.startswith(("http://", "https://")):
        with urllib.request.urlopen(source, timeout=60) as response:
            data = response.read(MAX_BYTES + 1)
    elif source.startswith("file://"):
        parsed = urllib.parse.urlparse(source)
        path = Path(urllib.request.url2pathname(parsed.path))
        data = path.read_bytes()
    else:
        path = Path(source)
        if not path.is_absolute():
            path = root / path
        data = path.read_bytes()
    if len(data) > MAX_BYTES:
        raise SystemExit(f"build-extensions: archive is too large: {source}")
    return data

def published_url(source: str, resolved: Path | None) -> str:
    if source.startswith(("http://", "https://")):
        return source
    docs = root / "docs"
    try:
        return resolved.relative_to(docs).as_posix()
    except ValueError as error:
        raise SystemExit(
            f"build-extensions: local source must live under docs/: {source}"
        ) from error

def find_manifest(archive: zipfile.ZipFile) -> bytes:
    candidates = []
    for name in archive.namelist():
        normalized = name.replace("\\", "/")
        if normalized == "extension.json":
            candidates.append((normalized, ""))
        elif normalized.count("/") == 1 and normalized.endswith("/extension.json"):
            candidates.append((normalized, normalized.split("/", 1)[0] + "/"))
    if len(candidates) != 1:
        raise SystemExit(
            "build-extensions: expected exactly one extension.json at the ZIP root "
            "or inside a single top-level directory"
        )
    name, _ = candidates[0]
    with archive.open(name) as file:
        return file.read()

entries = {}
for source in sources:
    resolved = None
    if source.startswith("file://"):
        parsed = urllib.parse.urlparse(source)
        resolved = Path(urllib.request.url2pathname(parsed.path)).resolve()
    elif not source.startswith(("http://", "https://")):
        resolved = (root / source).resolve()
    data = read_source(source)
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        manifest = json.loads(find_manifest(archive))
    required = ("id", "name", "version", "api_version")
    missing = [field for field in required if not manifest.get(field)]
    if missing:
        raise SystemExit(f"build-extensions: {source}: missing {', '.join(missing)}")
    devices = []
    for device in manifest.get("devices", []):
        type_id = device.get("type_id")
        if not type_id:
            raise SystemExit(f"build-extensions: {source}: device is missing type_id")
        devices.append({"name": device.get("name") or type_id, "type_id": type_id})
    entry = {
        "id": manifest["id"],
        "name": manifest["name"],
        "vendor": manifest.get("vendor", ""),
        "version": manifest["version"],
        "api_version": manifest["api_version"],
        "entrypoint": manifest.get("entrypoint") or manifest.get("entry") or "extension.wasm",
        "capabilities": manifest.get("capabilities", []),
        "devices": devices,
        "url": published_url(source, resolved),
    }
    entries[entry["id"]] = entry

out_path.parent.mkdir(parents=True, exist_ok=True)
out_path.write_text(json.dumps(list(entries.values()), indent=2, sort_keys=True) + "\n")
print(f"build-extensions: wrote {len(entries)} extension(s) to {out_path}")
PY
