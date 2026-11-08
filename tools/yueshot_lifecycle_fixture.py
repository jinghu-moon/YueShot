"""Explicit test-fixture generation for the installed A -> B lifecycle test."""
import argparse
from pathlib import Path
import sqlite3
import struct
import zlib


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=['png','future-schema'])
    parser.add_argument('path',type=Path)
    args=parser.parse_args()
    if not args.path.is_absolute():
        parser.error('fixture paths must be absolute')
    if args.command == 'png':
        def chunk(kind,data):
            return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data))
        data=(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',1,1,8,6,0,0,0))
              +chunk(b'IDAT',zlib.compress(b'\x00\xff\x00\x00\xff'))+chunk(b'IEND',b''))
        with args.path.open('xb') as file:
            file.write(data)
    else:
        with sqlite3.connect(args.path.as_uri()+'?mode=rw',uri=True) as database:
            if database.execute('pragma application_id').fetchone()[0] != 0x59534854:
                raise RuntimeError('refuse to change a database not owned by YueShot')
            database.execute('pragma user_version=999')


if __name__ == '__main__':
    main()
