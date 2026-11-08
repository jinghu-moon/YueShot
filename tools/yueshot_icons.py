"""Verify that Windows PE icon resources contain every supplied ICO image unchanged."""
import argparse
import collections
import ctypes
from ctypes import wintypes
import hashlib
import json
from pathlib import Path
import struct


def icon_images(data):
    if len(data) < 6:
        raise ValueError('truncated ICO header')
    reserved, kind, count = struct.unpack_from('<HHH', data)
    if reserved != 0 or kind != 1 or count == 0 or 6 + 16 * count > len(data):
        raise ValueError('invalid ICO directory')
    images = []
    for index in range(count):
        width, height, _, reserved, _, _, size, offset = struct.unpack_from(
            '<BBBBHHII', data, 6 + 16 * index)
        if reserved != 0 or size == 0 or offset < 6 + 16 * count or offset + size > len(data):
            raise ValueError('ICO image is outside its payload')
        images.append((width or 256, height or 256,
                       hashlib.sha256(data[offset:offset + size]).hexdigest()))
    return images


def verify(executable, icon):
    expected = icon_images(Path(icon).read_bytes())
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.LoadLibraryExW.argtypes = [wintypes.LPCWSTR, wintypes.HANDLE, wintypes.DWORD]
    kernel.LoadLibraryExW.restype = wintypes.HMODULE
    kernel.FindResourceW.argtypes = [wintypes.HMODULE, ctypes.c_void_p, ctypes.c_void_p]
    kernel.FindResourceW.restype = wintypes.HANDLE
    kernel.LoadResource.argtypes = [wintypes.HMODULE, wintypes.HANDLE]
    kernel.LoadResource.restype = wintypes.HANDLE
    kernel.SizeofResource.argtypes = [wintypes.HMODULE, wintypes.HANDLE]
    kernel.SizeofResource.restype = wintypes.DWORD
    kernel.LockResource.argtypes = [wintypes.HANDLE]
    kernel.LockResource.restype = ctypes.c_void_p
    kernel.FreeLibrary.argtypes = [wintypes.HMODULE]
    callback_type = ctypes.WINFUNCTYPE(
        wintypes.BOOL, wintypes.HMODULE, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_ssize_t)
    kernel.EnumResourceNamesW.argtypes = [wintypes.HMODULE, ctypes.c_void_p, callback_type, ctypes.c_ssize_t]
    path = Path(executable).resolve(strict=True)
    # Map resources as data: inspecting an installer never executes its code.
    module = kernel.LoadLibraryExW(str(path), None, 0x22)
    if not module:
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        names = []

        @callback_type
        def callback(_module, _kind, name, _parameter):
            names.append(name if name <= 65535 else ctypes.wstring_at(name))
            return True

        if not kernel.EnumResourceNamesW(module, ctypes.c_void_p(14), callback, 0):
            raise ctypes.WinError(ctypes.get_last_error())

        def resource(kind, name):
            pointer = ctypes.c_void_p(name) if isinstance(name, int) else ctypes.cast(
                ctypes.c_wchar_p(name), ctypes.c_void_p)
            item = kernel.FindResourceW(module, pointer, ctypes.c_void_p(kind))
            if not item:
                raise ctypes.WinError(ctypes.get_last_error())
            loaded = kernel.LoadResource(module, item)
            address = kernel.LockResource(loaded)
            size = kernel.SizeofResource(module, item)
            if not loaded or not address or size == 0:
                raise ValueError('empty Windows icon resource')
            return ctypes.string_at(address, size)

        groups = []
        for name in names:
            group = resource(14, name)
            if len(group) < 6:
                raise ValueError('truncated icon group')
            reserved, kind, count = struct.unpack_from('<HHH', group)
            if reserved != 0 or kind != 1 or len(group) != 6 + 14 * count:
                raise ValueError('invalid icon group')
            actual = []
            for index in range(count):
                width, height, _, _, _, _, size, image_id = struct.unpack_from(
                    '<BBBBHHIH', group, 6 + 14 * index)
                data = resource(3, image_id)
                if len(data) != size:
                    raise ValueError('icon group payload length differs')
                actual.append((width or 256, height or 256, hashlib.sha256(data).hexdigest()))
            if collections.Counter(actual) == collections.Counter(expected):
                groups.append(name)
        if not groups:
            raise RuntimeError(f'PE has no icon group matching all supplied ICO payloads: {path}')
        return {'image': path.name, 'iconSha256': hashlib.sha256(Path(icon).read_bytes()).hexdigest(),
                'matchingGroups': groups, 'sizes': [[w, h] for w, h, _ in expected]}
    finally:
        kernel.FreeLibrary(module)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('icon', type=Path)
    parser.add_argument('images', nargs='+', type=Path)
    args = parser.parse_args()
    print(json.dumps([verify(path, args.icon) for path in args.images], indent=2))
