fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=app-dev.manifest");

    let manifest = if std::env::var("ANYECHO_ADMIN_MANIFEST").is_ok() {
        include_str!("app.manifest")
    } else {
        include_str!("app-dev.manifest")
    };
    let attrs = tauri_build::Attributes::new().windows_attributes(
        tauri_build::WindowsAttributes::new().app_manifest(manifest),
    );
    tauri_build::try_build(attrs).expect("failed to run tauri-build");
}

