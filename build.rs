use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=icons/");
    println!("cargo:rerun-if-changed=assets/");
    println!("cargo:rerun-if-changed=igrisv3.rc");
    println!("cargo:rerun-if-changed=igrisv3.exe.manifest");
    println!("cargo:rerun-if-changed=proto/");
    println!("cargo:rerun-if-changed=src/core/piper_ffi.rs");

    // Use vendored protoc so Windows builds don't require manual protoc installation
    let proto_file = Path::new("proto/riva/proto/riva_asr.proto");
    if proto_file.exists() {
        let protoc_path = protoc_bin_vendored::protoc_bin_path()
            .expect("protoc-bin-vendored failed to locate protoc binary");
        std::env::set_var("PROTOC", protoc_path);

        tonic_build::configure()
            .build_server(false)
            .compile_protos(&["proto/riva/proto/riva_asr.proto"], &["proto/"])?;
    } else {
        println!("cargo:warning=proto file {} not found, skipping tonic_build", proto_file.display());
    }

    #[cfg(target_os = "windows")]
    {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
        let out_dir = std::env::var("OUT_DIR").unwrap_or_default();
        let manifest_path = std::path::Path::new(&manifest_dir);

        // Link libpiper (source-built piper1-gpl) when the FFI install is available.
        // When absent, the piper_ffi module is compiled out and TTS falls back
        // to spawning piper.exe.
        let piper_dir = manifest_path.join("pkg/piper/piper1-gpl/libpiper/install");
        if piper_dir.exists() {
            println!("cargo:rustc-link-search=native={}", piper_dir.display());
            println!("cargo:rustc-link-lib=piper");
            println!("cargo:rustc-cfg=piper_ffi");
        }

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
