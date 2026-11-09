"""Verify DPI awareness in the actual NSIS installer/uninstaller manifest."""
import argparse
import ctypes
from ctypes import wintypes
import json
from pathlib import Path
import xml.etree.ElementTree as ET


def check_manifest(data):
    root = ET.fromstring(data)
    setting = root.find('.//{http://schemas.microsoft.com/SMI/2005/WindowsSettings}dpiAware')
    modern = root.find('.//{http://schemas.microsoft.com/SMI/2016/WindowsSettings}dpiAwareness')
    if setting is None or (setting.text or '').strip().lower() != 'true':
        raise ValueError('NSIS must declare system DPI awareness; bitmap scaling is not allowed')
    if modern is not None and (modern.text or '').strip().lower() != 'system':
        raise ValueError('NSIS must not override system DPI awareness without layout support')
    return {'dpiAwareness': 'system'}


def verify(path):
    path = Path(path).resolve(strict=True)
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.LoadLibraryExW.argtypes = [wintypes.LPCWSTR, wintypes.HANDLE, wintypes.DWORD]
    kernel.LoadLibraryExW.restype = wintypes.HMODULE
    kernel.FindResourceW.argtypes = [wintypes.HMODULE, ctypes.c_void_p, ctypes.c_void_p]
    kernel.FindResourceW.restype = wintypes.HANDLE
    kernel.LoadResource.argtypes = [wintypes.HMODULE, wintypes.HANDLE]
    kernel.LoadResource.restype = wintypes.HANDLE
    kernel.LockResource.argtypes = [wintypes.HANDLE]
    kernel.LockResource.restype = ctypes.c_void_p
    kernel.SizeofResource.argtypes = [wintypes.HMODULE, wintypes.HANDLE]
    kernel.SizeofResource.restype = wintypes.DWORD
    kernel.FreeLibrary.argtypes = [wintypes.HMODULE]
    # Read resources only; never execute the image being checked.
    module = kernel.LoadLibraryExW(str(path), None, 0x22)
    if not module:
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        resource = kernel.FindResourceW(module, ctypes.c_void_p(1), ctypes.c_void_p(24))
        if not resource:
            raise ctypes.WinError(ctypes.get_last_error())
        loaded = kernel.LoadResource(module, resource)
        address = kernel.LockResource(loaded)
        size = kernel.SizeofResource(module, resource)
        if not loaded or not address or not size:
            raise ValueError('empty installer manifest')
        return {'image': path.name, **check_manifest(ctypes.string_at(address, size))}
    finally:
        kernel.FreeLibrary(module)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('images', nargs='+', type=Path)
    args = parser.parse_args()
    print(json.dumps([verify(path) for path in args.images], indent=2))
