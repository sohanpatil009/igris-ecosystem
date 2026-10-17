use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=icons/");
    println!("cargo:rerun-if-changed=assets/");
    println!("cargo:rerun-if-changed=igrisv3.rc");
    println!("cargo:rerun-if-changed=igrisv3.exe.manifest");
    // Note: the riva proto generation and libpiper link move with the
    // assistant brain into ../igris-ai (its build.rs). Only the runtime
    // DLL/data placement stays here — the assistant exe is this crate.

    #[cfg(target_os = "windows")]
    {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
        let out_dir = std::env::var("OUT_DIR").unwrap_or_default();
        let manifest_path = std::path::Path::new(&manifest_dir);

        // Possible output directories (cargo build, dx serve, etc.)
        let target_dirs = [
            manifest_path.join("target/debug"),
            manifest_path.join("target/release"),
            manifest_path.join("target/desktop-dev"),
            manifest_path.join("target/dx/igrisecosystem/debug/windows/app"),
            manifest_path.join("target/dx/igrisecosystem/release/windows/app"),
            manifest_path.to_path_buf(), // project root (dx serve CWD)
        ];

        // Copy DLLs to all possible output directories
        for dll in &["piper.dll", "onnxruntime.dll", "onnxruntime_providers_shared.dll"] {
            let src = manifest_path.join("pkg/piper").join(dll);
            if !src.exists() {
                continue;
            }

            for dir in &target_dirs {
                if dir.exists() {
                    let _ = std::fs::copy(&src, dir.join(dll));
                }
            }

            if let Some(deps) = std::path::Path::new(&out_dir).parent() {
                let _ = std::fs::copy(&src, deps.join(dll));
            }
        }

        // Copy espeak-ng-data directory to all output directories
        let espeak_src = manifest_path.join("pkg/piper/espeak-ng-data");
        if espeak_src.exists() {
            for dir in &target_dirs {
                if dir.exists() {
                    let dst = dir.join("pkg/piper/espeak-ng-data");
                    let _ = copy_dir_recursive(&espeak_src, &dst);
                }
            }
            if let Some(deps) = std::path::Path::new(&out_dir).parent() {
                let dst = deps.join("pkg/piper/espeak-ng-data");
                let _ = copy_dir_recursive(&espeak_src, &dst);
            }
        }

        let rc_path = Path::new("igrisv3.rc");
        if rc_path.exists() {
            embed_resource::compile(rc_path);
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        #[cfg(target_os = "macos")]
        {
            let icns_icon = Path::new("icons/igris_icon.icns");
            if icns_icon.exists() {
                println!("cargo:rustc-env=ICON_PATH={}", icns_icon.display());
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let svg_icon = Path::new("icons/igris_icon.svg");
            if svg_icon.exists() {
                println!("cargo:rustc-env=ICON_PATH={}", svg_icon.display());
            }
        }
    }

    Ok(())
}

/// Recursively copy a directory
#[cfg(target_os = "windows")]
fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            let _ = std::fs::copy(&src_path, &dst_path);
        }
    }
    Ok(())
}
