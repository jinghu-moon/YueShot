"""Native-process regression for close-to-tray; run on an interactive Windows desktop.

Only the spawned test process uses test data. The surviving process is terminated
in finally as test cleanup; this does not claim to test graceful tray-menu Exit.
"""
import argparse
import ctypes
from ctypes import wintypes
import json
from pathlib import Path
import subprocess
import time


def run(binary, output):
    user32 = ctypes.WinDLL('user32', use_last_error=True)
    callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    user32.EnumWindows.argtypes = [callback_type, wintypes.LPARAM]
    user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
    user32.GetClassNameW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
    user32.GetWindowTextW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
    user32.IsWindowVisible.argtypes = [wintypes.HWND]
    user32.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]

    def windows(pid):
        found = []
        @callback_type
        def collect(hwnd, _):
            owner = wintypes.DWORD()
            user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
            if owner.value == pid:
                name = ctypes.create_unicode_buffer(256)
                title = ctypes.create_unicode_buffer(256)
                user32.GetClassNameW(hwnd, name, len(name))
                user32.GetWindowTextW(hwnd, title, len(title))
                found.append((hwnd, name.value, title.value, bool(user32.IsWindowVisible(hwnd))))
            return True
        user32.EnumWindows(collect, 0)
        return found

    def wait(process, condition, message):
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            result = condition(windows(process.pid))
            if result:
                return result
            if process.poll() is not None:
                raise AssertionError(f'{message}: process exited {process.returncode}')
            time.sleep(.05)
        raise AssertionError(message)

    output = output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    report = {'binary': str(binary), 'checks': [], 'status': 'running',
              'scope': 'native WM_CLOSE, process survival, tray window and same-instance reopening; test-owned process termination is cleanup only'}
    try:
        for close_to_tray in [True, False]:
            data = output / ('tray' if close_to_tray else 'exit')
            data.mkdir()
            (data/'settings.json').write_text(json.dumps({'schemaVersion': 1, 'closeToTray': close_to_tray}), encoding='utf8')
            with (data/'process.log').open('wb') as log:
                process = subprocess.Popen([str(binary), '--show', '--data-dir', str(data)], stdout=log, stderr=log)
                try:
                    main = wait(process, lambda ws: next((w[0] for w in ws if w[2] == 'YueShot' and w[3] and not w[1].endswith('.Tray')), None), 'main window did not appear')
                    wait(process, lambda ws: any(w[1].endswith('.Tray') for w in ws), 'native tray window did not appear')
                    assert user32.PostMessageW(main, 0x0010, 0, 0), 'posting WM_CLOSE failed'
                    if close_to_tray:
                        wait(process, lambda ws: not any(w[0] == main for w in ws), 'main window did not close')
                        time.sleep(1)
                        assert process.poll() is None, 'closing last window exited the resident process'
                        assert any(w[1].endswith('.Tray') for w in windows(process.pid)), 'tray window disappeared'
                        forwarded = subprocess.run([str(binary), '--show', '--data-dir', str(data)], stdout=log, stderr=log, timeout=20)
                        assert forwarded.returncode == 0, 'same-instance Show command failed'
                        wait(process, lambda ws: any(w[2] == 'YueShot' and w[3] for w in ws), 'resident instance did not reopen its main window')
                        report['checks'].append('closeToTray=true: native close removes last GPUI window, process/tray survive and same-instance Show reopens')
                    else:
                        assert process.wait(timeout=20) == 0, 'closeToTray=false must exit normally'
                        assert not windows(process.pid), 'exit left native windows behind'
                        report['checks'].append('closeToTray=false: native close exits normally and removes tray')
                finally:
                    if process.poll() is None:
                        process.kill()  # Only this harness-owned child; no other process is touched.
                        process.wait(timeout=10)
        report['status'] = 'passed'
    except Exception as error:
        report['status'] = 'failed'
        report['error'] = str(error)
        raise
    finally:
        (output/'report.json').write_text(json.dumps(report, indent=2)+'\n', encoding='utf8')
        print(json.dumps(report, indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    run(args.binary.resolve(strict=True), args.output)
