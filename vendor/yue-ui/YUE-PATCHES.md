# YueShot snapshot adaptations

The retained snapshot's source bytes match the Yue-GPUI-Component checkout listed in `snapshot.json`, except for the workspace-root member list, local adaptations and explicit source exclusions recorded below and in `snapshot.json`:

- `vendor/gpui-pre/resources/windows/gpui.manifest.xml`: preserve the upstream PerMonitorV2, asInvoker, Windows 10, and SegmentHeap declarations; add `longPathAware=true` for installed product paths.

The application adds only its icon and version resource. The GPUI host remains the single owner of the process manifest resource.


Graphical SVG adaptation: resvg/usvg 0.49.0 enables `raster-images` only;
`text`, `system-fonts` and `memmap-fonts` are disabled. The host no longer
discovers fonts, loads bundled SVG fonts or shapes SVG text. The fontdb,
harfrust and direct skrifa paths have been removed from this host.
SVG text/tspan/textPath input returns `SvgError::UnsupportedText`, including
the image-cache and alpha-mask entry points; assets with text must be
converted to paths at authoring time. XML is parsed once using usvg's
re-exported roxmltree and passed to `Tree::from_xmltree`.
Paths, gradients, clipping, filters, embedded raster images, alpha conversion,
size limits and rasterization are retained. Ordinary UI text uses the Windows
DirectWrite backend and is independent of this SVG capability restriction.
Both current product consumers use graphical SVG assets; this snapshot does
not promise general SVG text viewing. The independent upstream Yue checkout
is not modified by this product adaptation. Future consumers that require SVG
text need an explicit design and dependency review, rather than enabling font
features through Cargo's feature union.
The separate GPUI Base time adaptation uses its existing web-time dependency
instead of the unmaintained instant crate. See snapshot.json localAdaptations.

Component font bundle exclusion: the product snapshot removes all six unused
TTFs from `crates/yue-component/assets/fonts` (four HarmonyOS faces,
TCloudNumber and the unused full Fira Code copy). Original paths and SHA-256
remain in `files` and `excludedFiles`; restoring a file, losing provenance or
deleting any unrecorded source fails snapshot verification. No font files are
modified, and no source redistribution rights are inferred for excluded fonts.
The optional `bundled-fonts` feature, embedded loading API and conditional font
branches are removed. Effective UI/number families remain Segoe UI with
Microsoft YaHei UI fallback; code uses Consolas with Courier New fallback,
matching both products' previously selected no-default-features behavior.
Upstream theme token data remains intact; effective role styles select system
families. Overlay Noto/Fira subsets and their licensed build sources are
separate application assets and remain unchanged. Only this controlled product
snapshot is curated; the independent component checkout remains untouched.
