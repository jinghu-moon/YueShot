use std::{env, fs, path::PathBuf};

fn pascal_case(name: &str) -> String {
    name.split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            let first = chars
                .next()
                .expect("nonempty name part")
                .to_ascii_uppercase();
            format!("{first}{}", chars.collect::<String>())
        })
        .collect()
}

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let assets = manifest.join("assets/tabler");
    let mut generated = String::from("// Generated from assets/tabler by build.rs.\n");
    let mut counts = Vec::new();

    for style in ["outline", "filled"] {
        let style_dir = assets.join(style);
        println!("cargo:rerun-if-changed={}", style_dir.display());
        generated.push_str(&format!("pub mod {style} {{\n"));
        generated.push_str("    use super::IconDefinition;\n");
        let mut files: Vec<_> = fs::read_dir(&style_dir)
            .expect("Tabler asset directory is missing; run tools/sync-tabler-icons.ps1")
            .map(|entry| entry.expect("read Tabler asset entry").path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "svg"))
            .collect();
        files.sort();
        counts.push((style, files.len()));
        let mut names = std::collections::HashSet::new();
        for file in files {
            println!("cargo:rerun-if-changed={}", file.display());
            let stem = file.file_stem().unwrap().to_str().expect("UTF-8 filename");
            let name = pascal_case(stem);
            assert!(
                names.insert(name.clone()),
                "duplicate generated icon: {name}"
            );
            generated.push_str(&format!(
                "    #[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]\n    pub struct {name};\n\
                 impl IconDefinition for {name} {{\n\
                     fn name(self) -> &'static str {{ \"{stem}\" }}\n\
                     fn style(self) -> &'static str {{ \"{style}\" }}\n\
                     fn svg(self) -> &'static [u8] {{ include_bytes!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/assets/tabler/{style}/{stem}.svg\")) }}\n\
                 }}\n"
            ));
        }
        generated.push_str("}\n");
    }
    generated.push_str(&format!(
        "pub const OUTLINE_COUNT: usize = {};\npub const FILLED_COUNT: usize = {};\n",
        counts[0].1, counts[1].1
    ));

    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("icons.rs");
    fs::write(out, generated).expect("write generated Tabler icon API");
}
