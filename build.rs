use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Tell Cargo that if the given file changes, to rerun this build script.
    //println!("cargo:rerun-if-changed=assets/*");
    let out_dir = env::var("OUT_DIR").unwrap();
    Command::new("cp")
        .arg("-R")
        .arg("assets")
        .arg(out_dir)
        .output()
        .ok();

    // Tell cargo to look for shared libraries in the specified directory
    // println!("cargo:rustc-link-search=/path/to/lib");

    // Tell cargo to tell rustc to link the system bzip2
    // shared library.
    // println!("cargo:rustc-link-lib=bz2")
    let prefix = env::var("HOMEBREW_PREFIX").unwrap();
    println!("cargo:rustc-link-arg=-L{prefix}/lib");

    if !env::var("TARGET").unwrap().contains("wasm") {
        println!("cargo:rustc-env=MACOSX_DEPLOYMENT_TARGET=15.2");
        // Compile C code
        println!("cargo:rerun-if-changed=gfxlowlevel.c");
        println!("cargo:rerun-if-changed=gfxlowlevel.h");
        cc::Build::new()
            .file("src/gfxlowlevel.c")
            .include("/opt/homebrew/include")
            .compile("gfxlowlevel");
        println!("cargo:rustc-link-search=native=/opt/homebrew/lib");
        println!("cargo:rustc-link-lib=dylib=sdl2");
        println!("cargo:rustc-link-lib=dylib=placebo");
        println!("cargo:rustc-link-lib=dylib=avformat");
        println!("cargo:rustc-link-lib=dylib=MoltenVk");
        let bindings = bindgen::Builder::default()
            // The input header we would like to generate
            // bindings for.
            .header("src/gfxlowlevel.h")
            .clang_arg("-I/opt/homebrew/include")
            .allowlist_function("^gfx_lowlevel_.*")
            .allowlist_type("^gfx_lowlevel_.*")
            .allowlist_item("GFX_EAGAIN")
            .allowlist_type("pl_var_type")
            .allowlist_item("^PL_VAR_.*")
            // Tell cargo to invalidate the built crate whenever any of the
            // included header files changed.
            .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
            // Finish the builder and generate the bindings.
            .generate()
            // Unwrap the Result and panic on failure.
            .expect("Unable to generate bindings");
        let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
        bindings
            .write_to_file(out_path.join("gfx_lowlevel_bindings.rs"))
            .expect("Couldn't write bindings!");

        // Tell cargo to invalidate the built crate whenever the wrapper changes
        println!("cargo:rerun-if-changed=wrapper.h");

        // The bindgen::Builder is the main entry point
        // to bindgen, and lets you build up options for
        // the resulting bindings.
        let bindings = bindgen::Builder::default()
            // The input header we would like to generate
            // bindings for.
            .header("wrapper.h")
            // Tell cargo to invalidate the built crate whenever any of the
            // included header files changed.
            .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
            // Finish the builder and generate the bindings.
            .generate()
            // Unwrap the Result and panic on failure.
            .expect("Unable to generate bindings");

        // Write the bindings to the $OUT_DIR/bindings.rs file.
        let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
        bindings
            .write_to_file(out_path.join("bindings.rs"))
            .expect("Couldn't write bindings!");
    }
}
