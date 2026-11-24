#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LIST="${EXTENSIONS_LIST:-$ROOT/docs/extensions.csv}"
OUT="${EXTENSIONS_OUTPUT:-$ROOT/docs/extensions.json}"

python3 - "$ROOT" "$LIST" "$OUT" <<'PY'
import csv
import hashlib
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

logical_lines = [
    line
    for line in list_path.read_text().splitlines()
    if line.strip() and not line.strip().startswith("#")
]
rows = list(csv.reader(logical_lines))
if not rows:
    raise SystemExit("build-extensions: list is empty")
header = [cell.strip().lower() for cell in rows[0]]
if header != ["source", "hash"]:
    raise SystemExit(
        "build-extensions: expected a CSV header with exactly: source,hash"
    )

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

def find_manifest(archive: zipfile.ZipFile) -> tuple[bytes, str]:
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
    name, base = candidates[0]
    with archive.open(name) as file:
        return file.read(), base


def read_icon(archive: zipfile.ZipFile, manifest: dict, base: str, source: str) -> bytes | None:
    image = manifest.get("image") or "icon.png"
    if not isinstance(image, str):
        raise SystemExit(f"build-extensions: {source}: image must be a string")
    normalized = image.replace("\\", "/").lstrip("/")
    if not normalized or normalized.startswith("../") or "/../" in normalized:
        raise SystemExit(f"build-extensions: {source}: image must stay inside the package")
    name = f"{base}{normalized}"
    try:
        with archive.open(name) as file:
            return file.read()
    except KeyError:
        return None


def icon_file_name(extension_id: str, source: str) -> str:
    if (
        not extension_id
        or extension_id in (".", "..")
        or "/" in extension_id
        or "\\" in extension_id
        or "\0" in extension_id
    ):
        raise SystemExit(
            f"build-extensions: {source}: extension id is not safe as a file name"
        )
    return f"{extension_id}.png"

def manifest_targets(manifest: dict, source: str) -> list[str]:
    entrypoints = manifest.get("entrypoints")
    if not isinstance(entrypoints, list) or not entrypoints:
        raise SystemExit(
            f"build-extensions: {source}: manifest must declare a non-empty entrypoints list"
        )
    targets = []
    for entry in entrypoints:
        if not isinstance(entry, dict):
            raise SystemExit(f"build-extensions: {source}: invalid entrypoint")
        entrypoint = entry.get("entrypoint") or entry.get("file")
        target = entry.get("target")
        if not entrypoint or not target:
            raise SystemExit(
                f"build-extensions: {source}: entrypoint and target are required"
            )
        targets.append(target)
    return targets

entries = {}
for row in rows[1:]:
    if not row:
        continue
    if len(row) != 2:
        raise SystemExit(
            f"build-extensions: expected source,hash columns, got {len(row)}: {row}"
        )
    source = row[0].strip()
    expected_hash = row[1].strip().lower()
    if not source:
        raise SystemExit("build-extensions: source must not be empty")

    resolved = None
    if source.startswith("file://"):
        parsed = urllib.parse.urlparse(source)
        resolved = Path(urllib.request.url2pathname(parsed.path)).resolve()
    elif not source.startswith(("http://", "https://")):
        resolved = (root / source).resolve()

    data = read_source(source)
    actual_hash = hashlib.sha256(data).hexdigest()
    if expected_hash and expected_hash != actual_hash:
        raise SystemExit(
            f"build-extensions: {source}: hash mismatch "
            f"(expected {expected_hash}, got {actual_hash})"
        )

    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        manifest_bytes, manifest_base = find_manifest(archive)
        manifest = json.loads(manifest_bytes)
        icon_bytes = read_icon(archive, manifest, manifest_base, source)

    required = ("id", "name", "version", "api_version")
    missing = [field for field in required if not manifest.get(field)]
    if missing:
        raise SystemExit(f"build-extensions: {source}: missing {', '.join(missing)}")

    targets = manifest_targets(manifest, source)
    devices = []
    for device in manifest.get("devices", []):
        type_id = device.get("type_id")
        if not type_id:
            raise SystemExit(f"build-extensions: {source}: device is missing type_id")
        devices.append({"name": device.get("name") or type_id, "type_id": type_id})

    url = published_url(source, resolved)
    image_name = icon_file_name(manifest["id"], source)
    if icon_bytes is not None:
        icons_dir = root / "docs" / "extensions"
        icons_dir.mkdir(parents=True, exist_ok=True)
        (icons_dir / image_name).write_bytes(icon_bytes)
    image_url = f"extensions/{image_name}" if icon_bytes is not None else None
    entry = entries.get(manifest["id"])
    if entry is None:
        entry = {
            "id": manifest["id"],
            "name": manifest["name"],
            "vendor": manifest.get("vendor", ""),
            "version": manifest["version"],
            "api_version": manifest["api_version"],
            "capabilities": manifest.get("capabilities", []),
            "devices": devices,
            "description": manifest.get("description"),
            "image": image_url,
            "urls": [],
        }
        entries[manifest["id"]] = entry
    elif entry["version"] != manifest["version"]:
        raise SystemExit(
            f"build-extensions: {manifest['id']}: mixed versions "
            f"{entry['version']} and {manifest['version']}"
        )
    elif entry.get("image") != image_url:
        raise SystemExit(
            f"build-extensions: {manifest['id']}: mixed icon images"
        )

    if not any(item["url"] == url for item in entry["urls"]):
        entry["urls"].append(
            {
                "url": url,
                "hash": actual_hash,
                "size": len(data),
                "targets": targets,
            }
        )

out_path.parent.mkdir(parents=True, exist_ok=True)
out_path.write_text(json.dumps(list(entries.values()), indent=2, sort_keys=True) + "\n")
print(f"build-extensions: wrote {len(entries)} extension(s) to {out_path}")
PY
