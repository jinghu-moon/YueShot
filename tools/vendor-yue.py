"""Create once, or verify, the product snapshot and recorded source exclusions."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parent.parent
DEST = ROOT / "vendor/yue-ui"
ROOTS = ["crates/yue-color", "crates/yue-component", "crates/yue-component-icons",
         "vendor/gpui-base", "vendor/gpui-pre", "vendor/gpui-pre-windows"]
SKIP = {".git", "target", "__pycache__"}

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def files(root):
    return sorted(p for p in root.rglob("*") if p.is_file() and not SKIP.intersection(p.relative_to(root).parts))

def verify_snapshot(destination=DEST):
    manifest = json.loads((destination / "snapshot.json").read_text(encoding="utf8"))
    actual = {p.relative_to(destination).as_posix():sha(p) for p in files(destination)
              if p != destination / "snapshot.json"}
    adaptations = manifest.get("localAdaptations", {})
    exclusions = manifest.get("excludedFiles", {})
    if adaptations.keys() & exclusions.keys():
        raise RuntimeError("snapshot file cannot be both adapted and excluded")
    for name, item in exclusions.items():
        if (Path(name).is_absolute() or ".." in Path(name).parts
                or name not in manifest["files"]
                or item.get("upstreamSha256") != manifest["files"][name]
                or not item.get("reason", "").strip()):
            raise RuntimeError(f"snapshot exclusion lacks original provenance: {name}")
        if name in actual:
            raise RuntimeError(f"excluded snapshot file was restored: {name}")
    adapted = {}
    for name, item in adaptations.items():
        if name in manifest["files"] and item.get("upstreamSha256") != manifest["files"][name]:
            raise RuntimeError(f"local adaptation lacks the recorded upstream hash: {name}")
        adapted[name] = item["sha256"]
    if any(actual.get(name) != digest for name,digest in adapted.items()):
        raise RuntimeError("local snapshot adaptation changed without provenance update")
    base = {name:digest for name,digest in actual.items() if name not in adapted}
    upstream = {name:digest for name,digest in manifest["files"].items()
                if name not in adapted and name not in exclusions}
    if upstream != base:
        raise RuntimeError("upstream snapshot bytes or file inventory changed")
    return manifest, len(actual), len(exclusions)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, help="create from an explicit independent component checkout; refuses replacement")
    args = parser.parse_args()
    if args.source:
        source = args.source.resolve(strict=True)
        if DEST.exists():
            raise SystemExit("snapshot exists; review and move it aside explicitly before importing a replacement")
        head = subprocess.check_output(["git", "-C", str(source), "rev-parse", "HEAD"], text=True).strip()
        status = subprocess.check_output(["git", "-C", str(source), "status", "--porcelain=v1"], text=True)
        DEST.mkdir(parents=True)
        for name in ROOTS:
            for original in files(source / name):
                relative = original.relative_to(source)
                target = DEST / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(original, target)
        shutil.copyfile(source / "LICENSE", DEST / "LICENSE")
        shutil.copyfile(source / "Cargo.toml", DEST / "UPSTREAM-Cargo.toml")
        # Preserve all dependency manifests; adapt only the enclosing workspace's members.
        original = (source / "Cargo.toml").read_text(encoding="utf8")
        lines = original.splitlines(keepends=True)
        lines = ["members = [\"crates/yue-color\", \"crates/yue-component\", \"crates/yue-component-icons\"]\n" if line.startswith("members =") else line for line in lines]
        (DEST / "Cargo.toml").write_text("".join(lines), encoding="utf8", newline="\n")
        manifest = {"format":1, "source":"Yue-GPUI-Component", "gitHead":head,
                    "dirty":bool(status), "gitStatus":status, "includedRoots":ROOTS,
                    "adaptations":["Cargo.toml: workspace members limited to the three consumed crates; original retained as UPSTREAM-Cargo.toml"],
                    "files":{p.relative_to(DEST).as_posix():sha(p) for p in files(DEST)}}
        (DEST / "snapshot.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2)+"\n", encoding="utf8")
    manifest, count, excluded = verify_snapshot()
    # Fail on path escapes; every path dependency must be contained in the checkout.
    for path in ROOT.rglob("Cargo.toml"):
        if any(part in {"target", "target-verify", "refer", ".git", "rapid-ocr-rs"} for part in path.relative_to(ROOT).parts):
            continue
        data = tomllib.loads(path.read_text(encoding="utf8"))
        def check(value):
            if isinstance(value, dict):
                if isinstance(value.get("path"), str):
                    resolved = (path.parent / value["path"]).resolve()
                    if not resolved.is_relative_to(ROOT):
                        raise SystemExit(f"external path dependency: {path.relative_to(ROOT)}: {value['path']}")
                for child in value.values(): check(child)
            elif isinstance(value, list):
                for child in value: check(child)
        check(data)
    print(f"verified {count} snapshot files, {excluded} recorded exclusions; source {manifest['gitHead']}, dirty={manifest['dirty']}")

if __name__ == "__main__":
    main()
