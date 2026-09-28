fn main() {
    if pkg_config::probe_library("ngspice").is_err() {
        panic!(
            "could not find the 'ngspice' pkg-config library (ngspice.pc); \
             install the libngspice development package (e.g. libngspice0-dev on Debian/Ubuntu)"
        );
    }

    #[cfg(feature = "circt")]
    link_circt();
}

/// Links against CIRCT/MLIR and generates Rust bindings for its C API
/// (`circt-c/...`, `mlir-c/...` headers).
///
/// There are no pkg-config `.pc` files for CIRCT/LLVM, so discovery follows
/// the LLVM-ecosystem convention instead: a `CIRCT_DIR` env var. Two
/// layouts are supported:
///
/// - **Installed prefix**: `CIRCT_DIR` has `include/circt-c` and
///   `lib/libCIRCTCAPI*` directly under it (e.g. after `ninja install`
///   with `-DCMAKE_INSTALL_PREFIX`). `MLIR_DIR` overrides where MLIR's own
///   install lives if it's a separate prefix.
/// - **In-tree dev build**: `CIRCT_DIR` is a CIRCT git checkout (headers
///   under `include/circt-c`, static libs under `build/lib`, with LLVM's
///   `llvm` submodule and its own `llvm/build` alongside it). This is what
///   following CIRCT's own build instructions produces, and is detected
///   automatically when `CIRCT_DIR/lib` doesn't exist but
///   `CIRCT_DIR/build/lib` does.
///
/// This deliberately does not attempt to build CIRCT from source itself:
/// CIRCT is a multi-hour LLVM+CMake build, and baking that into
/// `cargo build` would make ordinary builds unpredictably slow. A
/// pre-built checkout/install is required either way.
#[cfg(feature = "circt")]
fn link_circt() {
    use std::path::PathBuf;

    println!("cargo:rerun-if-env-changed=CIRCT_DIR");
    println!("cargo:rerun-if-env-changed=MLIR_DIR");
    println!("cargo:rerun-if-changed=src/circt_ffi/wrapper.h");

    let circt_dir = std::env::var("CIRCT_DIR").unwrap_or_else(|_| {
        panic!(
            "the 'circt' feature is enabled but CIRCT_DIR is not set; \
             point it at a CIRCT install prefix or an in-tree CIRCT git \
             checkout (see docs/src for setup instructions). If MLIR's \
             headers/libs live in a separate prefix, also set MLIR_DIR."
        )
    });
    let circt_dir = PathBuf::from(circt_dir);

    let layout = Layout::detect(&circt_dir, std::env::var("MLIR_DIR").ok().map(PathBuf::from));

    for dir in &layout.include_dirs {
        if !dir.exists() {
            panic!(
                "expected CIRCT/MLIR headers at '{}' but that directory does \
                 not exist; check CIRCT_DIR/MLIR_DIR and that CIRCT/MLIR have \
                 actually been built (`ninja` in the relevant build dir)",
                dir.display()
            );
        }
    }

    for dir in &layout.lib_dirs {
        println!("cargo:rustc-link-search=native={}", dir.display());
    }

    match layout.kind {
        LayoutKind::InstalledPrefix => {
            // A clean install prefix has a small, predictable set of CAPI
            // libraries (often built shared); link exactly what this
            // project's hw+comb walk needs by name.
            for lib in [
                "CIRCTCAPIHW",
                "CIRCTCAPIComb",
                "CIRCTCAPIExportVerilog",
                "MLIRCAPIIR",
                "MLIRCAPIRegisterEverything",
            ] {
                link_lib(&layout.lib_dirs, lib);
            }
        }
        LayoutKind::InTreeDevBuild => {
            // An in-tree `ninja` build (no `ninja install`) produces static
            // archives with real circular dependencies between CIRCT/MLIR/
            // LLVM component libraries -- the CAPI libs above plus their
            // transitive closure, which isn't something worth hand-tracking
            // as CIRCT/LLVM's internal library graph evolves. Instead, link
            // every static archive found under the build dirs, wrapped in
            // `--start-group`/`--end-group` so the linker re-scans the
            // whole set until all symbols resolve regardless of ordering.
            link_all_archives_grouped(&layout.lib_dirs);
            // LLVM/MLIR's C++ runtime dependencies when statically linked.
            // Passed as raw link args (not `rustc-link-lib`) so they land
            // *after* the archive group above -- the linker resolves
            // symbols left to right, so libstdc++ before the archives that
            // need it leaves e.g. `std::__once_callable` undefined.
            for sys_lib in ["stdc++", "pthread", "dl", "m", "z", "tinfo"] {
                println!("cargo:rustc-link-arg=-l{sys_lib}");
            }
        }
    }

    let bindings = bindgen::Builder::default()
        .header("src/circt_ffi/wrapper.h")
        // This crate is edition 2024, which requires `unsafe extern` blocks.
        .rust_edition(bindgen::RustEdition::Edition2024)
        .clang_args(layout.include_dirs.iter().map(|d| format!("-I{}", d.display())))
        // No standalone `clang` binary is required to run bindgen (it links
        // libclang directly), but that means it also can't shell out to
        // `clang -print-resource-dir` to find bundled freestanding headers
        // like stdbool.h/stddef.h. GCC ships equivalent freestanding
        // headers (clang-compatible) under its own private include dir;
        // fall back to those when no clang resource dir is available.
        .clang_args(gcc_freestanding_include_dir().map(|d| format!("-I{}", d.display())))
        // Only the C API surface this project actually walks (core MLIR
        // context/module/operation/value plumbing, plus the hw/comb
        // dialects) -- keeps the generated bindings small.
        .allowlist_type("Mlir.*")
        .allowlist_function("mlir.*")
        .allowlist_function("hw.*")
        .allowlist_function("comb.*")
        .allowlist_var("Mlir.*")
        .derive_default(true)
        .generate()
        .expect(
            "bindgen failed to generate CIRCT/MLIR C API bindings; check \
             that CIRCT_DIR/MLIR_DIR point at a compatible CIRCT build",
        );

    let out_path = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("circt_bindings.rs"))
        .expect("failed to write generated CIRCT/MLIR bindings");
}

/// Finds GCC's private include dir (`gcc -print-file-name=include`), which
/// carries freestanding headers (`stdbool.h`, `stddef.h`, `stdarg.h`, ...)
/// libclang would otherwise expect from a full clang toolchain install.
#[cfg(feature = "circt")]
fn gcc_freestanding_include_dir() -> Option<std::path::PathBuf> {
    let output = std::process::Command::new("gcc")
        .arg("-print-file-name=include")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = std::path::PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    path.join("stdbool.h").exists().then_some(path)
}

#[cfg(feature = "circt")]
enum LayoutKind {
    InstalledPrefix,
    InTreeDevBuild,
}

#[cfg(feature = "circt")]
struct Layout {
    include_dirs: Vec<std::path::PathBuf>,
    lib_dirs: Vec<std::path::PathBuf>,
    kind: LayoutKind,
}

#[cfg(feature = "circt")]
impl Layout {
    fn detect(circt_dir: &std::path::Path, mlir_dir_override: Option<std::path::PathBuf>) -> Self {
        let installed_lib = circt_dir.join("lib");
        let installed_include = circt_dir.join("include");
        let dev_circt_build_lib = circt_dir.join("build").join("lib");

        if mlir_dir_override.is_none() && installed_lib.exists() && installed_lib.join("cmake").exists() {
            // Genuine install prefix (has CMake package config files, e.g.
            // MLIRConfig.cmake, alongside the libs).
            let mlir_dir = circt_dir.to_path_buf();
            return Layout {
                include_dirs: vec![installed_include, mlir_dir.join("include")],
                lib_dirs: vec![installed_lib, mlir_dir.join("lib")],
                kind: LayoutKind::InstalledPrefix,
            };
        }

        if dev_circt_build_lib.exists() {
            // In-tree CIRCT git checkout, built (not installed) following
            // CIRCT's own instructions: CIRCT's `llvm` submodule holds
            // LLVM/MLIR's source, built under `llvm/build`.
            let llvm_root = mlir_dir_override.unwrap_or_else(|| circt_dir.join("llvm"));
            let llvm_build = llvm_root.join("build");
            return Layout {
                include_dirs: vec![
                    circt_dir.join("include"),           // circt-c/... (source)
                    circt_dir.join("build").join("include"), // generated circt/*.capi.h.inc
                    llvm_root.join("mlir").join("include"),  // mlir-c/... (source)
                    llvm_root.join("llvm").join("include"),  // core llvm-c/... (source)
                    llvm_build.join("include"),               // generated llvm/mlir headers
                    llvm_build.join("tools").join("mlir").join("include"), // generated mlir-c/...
                ],
                lib_dirs: vec![dev_circt_build_lib, llvm_build.join("lib")],
                kind: LayoutKind::InTreeDevBuild,
            };
        }

        panic!(
            "could not find a CIRCT build under CIRCT_DIR='{}': expected \
             either '{}/lib' (installed prefix) or \
             '{}/build/lib' (in-tree dev build, `ninja` run inside CIRCT's \
             own `build` dir). Has CIRCT actually been built?",
            circt_dir.display(),
            circt_dir.display(),
            circt_dir.display(),
        );
    }
}

/// Emits a `cargo:rustc-link-lib` for `name` if a shared or static build of
/// it is found in any of `lib_dirs`; panics with a clear message otherwise,
/// since a silently-skipped link line just turns into a confusing
/// "undefined symbol" error much later at link time.
#[cfg(feature = "circt")]
fn link_lib(lib_dirs: &[std::path::PathBuf], name: &str) {
    let found = lib_dirs.iter().any(|dir| {
        [format!("lib{name}.so"), format!("lib{name}.dylib"), format!("lib{name}.a")]
            .iter()
            .any(|fname| dir.join(fname).exists())
    });
    if !found {
        panic!(
            "could not find lib{name}.{{so,dylib,a}} under any of {:?}; \
             this CIRCT build may be missing the {name} CAPI target",
            lib_dirs,
        );
    }
    println!("cargo:rustc-link-lib={name}");
}

/// Links every `.a` archive under `lib_dirs`, wrapped in
/// `-Wl,--start-group`/`-Wl,--end-group` (GNU ld/gold/lld all accept this)
/// so circular dependencies between CIRCT/MLIR/LLVM's many fine-grained
/// component libraries resolve regardless of link order. Used only for the
/// in-tree dev build layout, where there's no small curated set of
/// libraries to link by name.
#[cfg(feature = "circt")]
fn link_all_archives_grouped(lib_dirs: &[std::path::PathBuf]) {
    let mut archives = Vec::new();
    for dir in lib_dirs {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("a") {
                archives.push(path);
            }
        }
    }
    if archives.is_empty() {
        panic!(
            "no .a static archives found under any of {:?}; has CIRCT/MLIR \
             actually been built (`ninja` in the relevant build dir)?",
            lib_dirs,
        );
    }
    archives.sort();

    println!("cargo:rustc-link-arg=-Wl,--start-group");
    for archive in &archives {
        println!("cargo:rustc-link-arg={}", archive.display());
    }
    println!("cargo:rustc-link-arg=-Wl,--end-group");
}
