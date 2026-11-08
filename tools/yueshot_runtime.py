"""Audit YueShot's x64 PE imports without consulting the host DLL search path.

Windows provides the explicitly listed OS DLLs and API-set contracts. VC runtime
imports must be shipped app-local from the Visual Studio redistributable folder.
This checks static/delay imports, not runtime LoadLibrary calls or minimum OS APIs.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess


SYSTEM_DLLS = frozenset("""
advapi32.dll bcrypt.dll bcryptprimitives.dll comctl32.dll combase.dll
coremessaging.dll crypt32.dll d2d1.dll d3d11.dll d3d12.dll dcomp.dll
dwmapi.dll dwrite.dll dxgi.dll gdi32.dll hid.dll icu.dll icuuc.dll
imm32.dll iphlpapi.dll kernel32.dll kernelbase.dll msimg32.dll
mswsock.dll ncrypt.dll ntdll.dll ole32.dll oleacc.dll oleaut32.dll
powrprof.dll propsys.dll rpcrt4.dll secur32.dll setupapi.dll shcore.dll
shell32.dll shlwapi.dll uiautomationcore.dll user32.dll userenv.dll uxtheme.dll
version.dll windowscodecs.dll winhttp.dll winmm.dll winspool.drv
wintrust.dll ws2_32.dll wtsapi32.dll
""".split())
VC_DLLS = frozenset("""
concrt140.dll msvcp140.dll msvcp140_1.dll msvcp140_2.dll
msvcp140_atomic_wait.dll msvcp140_codecvt_ids.dll vccorlib140.dll
vcruntime140.dll vcruntime140_1.dll vcruntime140_threads.dll
""".split())


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def pe_imports(path):
    """Return x64 image identity and eager/delay DLL names; fail on malformed input."""
    data = Path(path).read_bytes()

    def unpack(fmt, offset):
        size = struct.calcsize(fmt)
        if offset < 0 or offset + size > len(data):
            raise ValueError("truncated PE image")
        return struct.unpack_from(fmt, data, offset)

    if data[:2] != b"MZ":
        raise ValueError("not a PE image")
    pe = unpack("<I", 0x3C)[0]
    if data[pe:pe + 4] != b"PE\0\0":
        raise ValueError("invalid PE signature")
    machine, count, _, _, _, optional_size, _ = unpack("<HHIIIHH", pe + 4)
    optional = pe + 24
    if machine != 0x8664 or unpack("<H", optional)[0] != 0x20B:
        raise ValueError("expected an x64 PE32+ image")
    if optional_size < 112:
        raise ValueError("truncated PE optional header")
    image_base = unpack("<Q", optional + 24)[0]
    header_size = unpack("<I", optional + 60)[0]
    subsystem = unpack("<H", optional + 68)[0]
    directories = unpack("<I", optional + 108)[0]
    if directories > (optional_size - 112) // 8:
        raise ValueError("PE directory count exceeds optional header")
    sections = []
    for ix in range(count):
        section = optional + optional_size + 40 * ix
        virtual_size, rva, raw_size, raw = unpack("<IIII", section + 8)
        sections.append((rva, virtual_size, raw_size, raw))

    def offset(rva, size):
        if 0 <= rva < header_size and rva + size <= min(header_size, len(data)):
            return rva
        for start, virtual_size, raw_size, raw in sections:
            delta = rva - start
            if 0 <= delta < max(virtual_size, raw_size) and delta + size <= raw_size:
                if raw + delta + size <= len(data):
                    return raw + delta
        raise ValueError("PE RVA is outside file-backed data")

    def name_at(rva):
        name = bytearray()
        for ix in range(260):
            char = data[offset(rva + ix, 1)]
            if char == 0:
                text = name.decode("ascii").lower()
                if not text or any(c in text for c in "/\\:"):
                    raise ValueError("invalid imported DLL name")
                return text
            name.append(char)
        raise ValueError("unterminated imported DLL name")

    def table(index, stride, delay):
        if directories <= index:
            return []
        rva, size = unpack("<II", optional + 112 + index * 8)
        if rva == 0 and size == 0:
            return []
        if not rva or size < stride:
            raise ValueError("invalid PE import directory")
        names = []
        for cursor in range(0, size - stride + 1, stride):
            words = unpack("<" + "I" * (stride // 4), offset(rva + cursor, stride))
            if not any(words):
                return sorted(set(names))
            if delay:
                if words[0] not in (0, 1):
                    raise ValueError("invalid PE delay-import attributes")
                name_rva = words[1] if words[0] else words[1] - image_base
            else:
                name_rva = words[3]
            names.append(name_at(name_rva))
        raise ValueError("unterminated PE import directory")

    return {"machine": "x64", "subsystem": subsystem,
            "imports": table(1, 20, False), "delayImports": table(13, 32, True)}


def is_system(name):
    return name in SYSTEM_DLLS or name.startswith(("api-ms-win-", "ext-ms-win-"))


def find_vc_redist():
    override = os.environ.get("YUESHOT_VC_REDIST_DIR")
    if override:
        folder = Path(override).resolve(strict=True)
        if folder.name != "Microsoft.VC143.CRT" or folder.parent.name != "x64":
            raise RuntimeError("YUESHOT_VC_REDIST_DIR must name an x64/Microsoft.VC143.CRT folder")
        return folder
    vswhere = Path(os.environ.get("ProgramFiles(x86)", "C:/Program Files (x86)")) / "Microsoft Visual Studio/Installer/vswhere.exe"
    installation = subprocess.check_output(
        [str(vswhere), "-latest", "-products", "*", "-requires",
         "Microsoft.VisualStudio.Component.VC.Tools.x86.x64", "-property", "installationPath"],
        text=True).strip()
    if installation:
        version_file = Path(installation) / "VC/Auxiliary/Build/Microsoft.VCRedistVersion.default.txt"
        version = version_file.read_text(encoding="utf-8-sig").strip()
        folder = Path(installation) / "VC/Redist/MSVC" / version / "x64/Microsoft.VC143.CRT"
        if folder.is_dir():
            return folder
    raise RuntimeError("official x64 VC redistributable folder not found; set YUESHOT_VC_REDIST_DIR")


def audit_runtime(executable, redist=None, bundle=False):
    """Audit only the staged image tree; optionally supply missing VC runtime files."""
    executable = Path(executable).resolve(strict=True)
    images, supplied = {}, {}
    pending = [executable]
    while pending:
        path = pending.pop()
        key = path.name.lower()
        if key in images:
            continue
        info = pe_imports(path)
        images[key] = {"sha256": digest(path), **info}
        for name in sorted(set(info["imports"] + info["delayImports"])):
            if is_system(name):
                continue
            if name not in VC_DLLS:
                raise RuntimeError(f"unreviewed non-system DLL import: {path.name} -> {name}")
            local = path.parent / name
            if not local.is_file():
                if not bundle:
                    raise RuntimeError(f"missing app-local runtime: {path.name} -> {name}")
                if redist is None:
                    redist = find_vc_redist()
                source = Path(redist) / name
                if not source.is_file():
                    raise RuntimeError("VC redistributable is missing " + name)
                pe_imports(source)  # Reject an incorrect architecture before staging.
                shutil.copyfile(source, local)
                supplied[name] = {"source": "Visual Studio VC/Redist/MSVC/" + "/".join(Path(redist).parts[-3:]),
                                  "sha256": digest(local)}
            pending.append(local)
    return {"format": 1, "architecture": "x64", "images": images,
            "suppliedRuntime": supplied,
            "limits": ["OS DLL/API-set availability and minimum Windows version require clean-machine validation",
                       "LoadLibrary/COM/driver dependencies are outside the static import audit",
                       "Font, third-party notice and Microsoft redistribution license review remain separate"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path)
    parser.add_argument("--bundle", action="store_true")
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    report = audit_runtime(args.executable, bundle=args.bundle)
    text = json.dumps(report, indent=2) + "\n"
    if args.report:
        args.report.write_text(text, encoding="utf8")
    else:
        print(text, end="")


if __name__ == "__main__":
    main()
