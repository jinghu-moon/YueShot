# Original codec source archives

These are unmodified crates.io release archives for the three locally patched AVIF dependencies. SHA-256, source URL, original license declaration, omissions and patch hashes are recorded in tools/codec-bench/hdr-avif-sources.json.

Cargo fetch downloads registry dependencies but does not download the original archives for path-patched crates. Keep these originals in the controlled source tree so source/license verification and corresponding-source packaging work on a clean machine without an old Cargo cache. No installed product loads these archives.

tools/check-hdr-codec-vendor.py verifies the original checksum and complete retained file inventory against the reviewed patch. tools/check-hdr-codec-licenses.py includes the same verified originals in the optional codec source bundle. Each archive retains its upstream license texts; this directory does not change their license.
