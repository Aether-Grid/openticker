use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir = env::var_os("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR is always set for crate build scripts");
    let ui_dir = Path::new(&manifest_dir).join("ui");
    let dist_dir = ui_dir.join(".output").join("public");
    let embedded_shell = dist_dir.join("index.html");

    assert!(
        embedded_shell.exists(),
        "missing Nuxt SPA shell at {}. Run `pnpm install && pnpm build` in {}",
        embedded_shell.display(),
        ui_dir.display()
    );

    for path in [
        ui_dir.join("package.json"),
        ui_dir.join("pnpm-lock.yaml"),
        ui_dir.join("nuxt.config.ts"),
        ui_dir.join("tsconfig.json"),
        ui_dir.join("app"),
        ui_dir.join("public"),
        dist_dir.clone(),
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
