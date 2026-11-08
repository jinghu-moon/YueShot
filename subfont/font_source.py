"""Pinned source verification and legal metadata shared by both font builders."""
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SOURCES = ROOT / 'tools/delivery/licenses/sources.json'
NAME_IDS = '0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,16,17'


def verify_source(path: Path, entry: str) -> None:
    import fontTools
    if fontTools.__version__ != '4.55.6':
        raise RuntimeError('install pinned fontTools: pip install -r subfont/requirements.txt')
    expected = json.loads(SOURCES.read_text(encoding='utf8'))[entry]['fontSha256']
    if hashlib.sha256(path.read_bytes()).hexdigest() != expected:
        raise RuntimeError(f'font source checksum mismatch: {path.relative_to(ROOT)}')


def verify_legal_names(source, built) -> None:
    for name_id in (0, 13, 14):
        original = {n.toUnicode() for n in source['name'].names if n.nameID == name_id}
        retained = {n.toUnicode() for n in built['name'].names if n.nameID == name_id}
        if not original or not original.issubset(retained):
            raise RuntimeError(f'font copyright/license metadata lost: name ID {name_id}')
