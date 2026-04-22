use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir = env::var_os("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR is always set for crate build scripts");
    let static_dir = Path::new(&manifest_dir).join("static");
    let embedded_dashboard = static_dir.join("dist").join("index.html");

    assert!(
        embedded_dashboard.exists(),
        "missing embedded dashboard asset at {}. run `npm ci` and `npm run build` in {}",
        embedded_dashboard.display(),
        static_dir.display()
    );

    for path in [
        static_dir.join("package.json"),
        static_dir.join("package-lock.json"),
        static_dir.join("astro.config.mjs"),
        static_dir.join("tsconfig.json"),
        static_dir.join("src"),
        static_dir.join("public"),
        static_dir.join("dist"),
    ] {
        emit_rerun_if_changed(&path);
    }
}

fn emit_rerun_if_changed(path: &Path) {
    if !path.exists() {
        return;
    }

    if path.is_dir() {
        let entries = fs::read_dir(path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        for entry in entries {
            let entry =
                entry.unwrap_or_else(|error| panic!("failed to walk {}: {error}", path.display()));
            emit_rerun_if_changed(&entry.path());
        }
        return;
    }

    println!("cargo:rerun-if-changed={}", path.display());
}
