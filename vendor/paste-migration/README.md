# Maintained macro dependency

`pulp 0.22.3` (EXR SIMD) and `garb 0.2.8` (WebP and AVIF pixel conversion)
come from their original crates.io archives. `sources.json` records the archive
SHA-256 values from Cargo.lock before patching, the retained archives, and the
upstream commits that migrate each consumer away from `paste`.

Only `Cargo.toml` and `Cargo.toml.orig` change: both consumers use the dependency
alias `paste = { package = "pastey", version = "=0.2.3" }`. This follows garb's
upstream approach. Pulp's upstream commit instead renames its source imports to
`pastey`; retaining the alias here avoids changing any SIMD source or macro calls.
Garb's optional dependency and `experimental` feature remain unchanged.

No upstream Rust source, tests, licenses, image formats, or feature defaults are
modified. `python tools/yueshot_vendor.py` compares every extracted file against
the retained archives with exactly these manifest substitutions. The original
licenses and modified sources travel in YueShot's corresponding-source archive.

Keep these patches until published consumer versions incorporate the maintained
macro dependency and pass product regression. Do not suppress RustSec advisories
or relabel paste itself as a maintained package.
