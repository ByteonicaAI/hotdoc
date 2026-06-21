use std::env;
use std::fs;
use std::path::Path;

fn main() {
    tauri_build::build();

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set");
    let repo_root = Path::new(&manifest_dir).join("..");
    let src_packs = repo_root.join("packs").join("curate");
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR set");
    let dest_dir = Path::new(&out_dir).join("bundled-packs");
    fs::create_dir_all(&dest_dir).expect("create bundled-packs dir");

    if src_packs.is_dir() {
        let mut count = 0;
        for entry in fs::read_dir(&src_packs).expect("read packs/curate") {
            let entry = entry.expect("dir entry");
            let p = entry.path();
            if !p.is_file() || p.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let Some(name) = p.file_name() else {
                continue;
            };
            let dest = dest_dir.join(name);
            fs::copy(&p, &dest).expect("copy pack JSON");
            count += 1;
        }
        println!("cargo:warning=bundled {count} pack file(s) into {}", dest_dir.display());
    } else {
        println!("cargo:warning=source packs dir missing: {}", src_packs.display());
    }

    println!("cargo:rerun-if-changed={}", src_packs.display());
}
