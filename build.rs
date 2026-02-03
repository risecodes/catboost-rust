extern crate bindgen;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn get_catboost_version() -> String {
    env::var("CATBOOST_VERSION").unwrap_or_else(|_| "1.2.8".to_string())
}

fn get_platform_info() -> (String, String) {
    let target = env::var("TARGET").unwrap();

    // Determine OS
    let os = if target.contains("apple-darwin") {
        "darwin"
    } else if target.contains("linux") {
        "linux"
    } else if target.contains("windows") {
        "windows"
    } else {
        panic!("Unsupported target: {}", target);
    };

    // Determine architecture
    let arch = if target.contains("x86_64") {
        "x86_64"
    } else if target.contains("aarch64") || target.contains("arm64") {
        "aarch64"
    } else if target.contains("i686") || target.contains("i586") {
        "i686"
    } else {
        panic!("Unsupported architecture for target: {}", target);
    };

    (os.to_string(), arch.to_string())
}

fn download_model_interface_headers(out_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Create the model_interface directory
    let model_interface_dir = out_dir.join("libs/model_interface");
    fs::create_dir_all(&model_interface_dir)?;

    // Use bundled c_api.h file (hardcoded for testing)
    let manifest_dir = std::path::Path::new(file!()).parent().unwrap();
    let bundled_c_api = manifest_dir.join("c_api.h");
    let c_api_path = model_interface_dir.join("c_api.h");

    println!("cargo:warning=Using bundled c_api.h from: {}", bundled_c_api.display());

    fs::copy(&bundled_c_api, &c_api_path)?;

    Ok(())
}

fn download_compiled_library(out_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let (os, _arch) = get_platform_info();

    // Create the library directory
    let lib_dir = out_dir.join("libs");
    fs::create_dir_all(&lib_dir)?;

    // Use bundled library file based on target platform (hardcoded for testing)
    let manifest_dir = std::path::Path::new(file!()).parent().unwrap();

    // Determine source and target filenames based on OS
    let (bundled_lib, lib_filename) = match os.as_str() {
        "windows" => (
            manifest_dir.join("catboostmodel.dll"),
            "catboostmodel.dll"
        ),
        "darwin" => (
            manifest_dir.join("libcatboostmodel.dylib"),
            "libcatboostmodel.dylib"
        ),
        _ => (
            manifest_dir.join("libcatboostmodel.so"),
            "libcatboostmodel.so"
        ),
    };

    let lib_path = lib_dir.join(lib_filename);

    println!("cargo:warning=Using bundled library from: {}", bundled_lib.display());

    fs::copy(&bundled_lib, &lib_path)?;

    println!(
        "cargo:warning=Copied CatBoost library to: {}",
        lib_path.display()
    );

    Ok(())
}

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let cb_model_interface_root = out_dir.join("libs/model_interface");

    // Declare custom cfg flags for Cargo's check-cfg feature
    println!("cargo::rustc-check-cfg=cfg(catboost_embeddings)");
    println!("cargo::rustc-check-cfg=cfg(catboost_text_count)");
    println!("cargo::rustc-check-cfg=cfg(catboost_staged_prediction)");
    println!("cargo::rustc-check-cfg=cfg(catboost_feature_indices)");

    // Parse version for feature detection
    let version = get_catboost_version();
    let version_parts: Vec<&str> = version.split('.').collect();
    let major: u32 = version_parts
        .first()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let minor: u32 = version_parts
        .get(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let patch: u32 = version_parts
        .get(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    // Emit cfg flags for version-specific features
    // v1.1.1+: Embedding features support
    if major > 1 || (major == 1 && minor > 1) || (major == 1 && minor == 1 && patch >= 1) {
        println!("cargo:rustc-cfg=catboost_embeddings");
    }

    // v1.2+: Text features count function
    if major > 1 || (major == 1 && minor >= 2) {
        println!("cargo:rustc-cfg=catboost_text_count");
    }

    // v1.2.3+: Staged predictions and feature indices
    if major > 1 || (major == 1 && minor > 2) || (major == 1 && minor == 2 && patch >= 3) {
        println!("cargo:rustc-cfg=catboost_staged_prediction");
        println!("cargo:rustc-cfg=catboost_feature_indices");
    }

    // Download the model interface headers
    if let Err(e) = download_model_interface_headers(&out_dir) {
        eprintln!("Failed to download model interface headers: {}", e);
        panic!("Cannot proceed without headers");
    }

    // Download the compiled library
    if let Err(e) = download_compiled_library(&out_dir) {
        eprintln!("Failed to download compiled library: {}", e);
        panic!("Cannot proceed without compiled library");
    }

    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_arg(format!("-I{}", cb_model_interface_root.display()))
        .size_t_is_usize(true)
        .generate()
        .expect("Unable to generate bindings.");

    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("Couldn't write bindings.");

    // 1. Get platform info using your existing function
    let (os, _arch) = get_platform_info();

    // 2. Determine the library filename based on the OS
    let lib_filename = match os.as_str() {
        "windows" => "catboostmodel.dll",
        "darwin" => "libcatboostmodel.dylib", // "darwin" comes from your function
        _ => "libcatboostmodel.so",           // Default to Linux/Unix
    };

    // 3. Copy the library from OUT_DIR/libs to the final target directory
    let lib_source_path = out_dir.join("libs").join(lib_filename);

    // Find the final output directory (e.g., target/release)
    let target_dir = out_dir
        .ancestors()
        .find(|p| p.ends_with("target"))
        .unwrap()
        .join(env::var("PROFILE").unwrap());

    let lib_dest_path = target_dir.join(lib_filename);
    fs::copy(&lib_source_path, &lib_dest_path).expect("Failed to copy library to target directory");

    // On macOS/Linux, change the install name/soname to use @loader_path/$ORIGIN
    // This needs to be done on the source library in OUT_DIR before linking
    if os == "darwin" {
        use std::process::Command;
        let _ = Command::new("install_name_tool")
            .arg("-id")
            .arg(format!("@loader_path/{}", lib_filename))
            .arg(&lib_source_path)
            .status();
        // Also update the copy
        let _ = Command::new("install_name_tool")
            .arg("-id")
            .arg(format!("@loader_path/{}", lib_filename))
            .arg(&lib_dest_path)
            .status();
    } else if os == "linux" {
        use std::process::Command;
        // Use patchelf to set soname to just the library filename on Linux (if available)
        // This is optional - if patchelf is not installed, we just skip it
        let _ = Command::new("patchelf")
            .arg("--set-soname")
            .arg(lib_filename)
            .arg(&lib_source_path)
            .output(); // Use output() to silently ignore if patchelf doesn't exist
        let _ = Command::new("patchelf")
            .arg("--set-soname")
            .arg(lib_filename)
            .arg(&lib_dest_path)
            .output();
    }

    // 4. Set the library search path for the build-time linker
    let lib_search_path = out_dir.join("libs");
    println!(
        "cargo:rustc-link-search=native={}",
        lib_search_path.display()
    );

    // 5. Set the rpath for the run-time linker based on the OS
    match os.as_str() {
        "darwin" => {
            // For macOS, add multiple rpath entries for IDE compatibility
            println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path");
            println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../..");
            println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path");
            println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path/../..");
            println!(
                "cargo:rustc-link-arg=-Wl,-rpath,{}",
                lib_search_path.display()
            );
            // Add the target directory to rpath as well
            if let Some(target_root) = out_dir.ancestors().find(|p| p.ends_with("target")) {
                println!(
                    "cargo:rustc-link-arg=-Wl,-rpath,{}/debug",
                    target_root.display()
                );
                println!(
                    "cargo:rustc-link-arg=-Wl,-rpath,{}/release",
                    target_root.display()
                );
            }
        }
        "linux" => {
            // For Linux, use $ORIGIN
            println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
            println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/../..");
            println!(
                "cargo:rustc-link-arg=-Wl,-rpath,{}",
                lib_search_path.display()
            );
            // Add the target directory to rpath as well
            if let Some(target_root) = out_dir.ancestors().find(|p| p.ends_with("target")) {
                println!(
                    "cargo:rustc-link-arg=-Wl,-rpath,{}/debug",
                    target_root.display()
                );
                println!(
                    "cargo:rustc-link-arg=-Wl,-rpath,{}/release",
                    target_root.display()
                );
            }
        }
        _ => {} // No rpath needed for Windows
    }

    println!("cargo:rustc-link-lib=dylib=catboostmodel");
}
