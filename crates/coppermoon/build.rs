//! Build script for CopperMoon
//!
//! On Windows, builds lua54.dll from the vendored Lua source.
//! This DLL is required for native module support — modules compiled with
//! mlua's "module" feature use raw_dylib linking that expects lua54.dll at runtime.

fn main() {
    // On Linux, export Lua symbols from the binary so that native modules
    // (.so cdylib) loaded via dlopen can resolve lua_type, lua_pushstring, etc.
    #[cfg(target_os = "linux")]
    println!("cargo:rustc-link-arg=-Wl,--export-dynamic");

    #[cfg(windows)]
    build_lua_shared();
}

#[cfg(windows)]
fn build_lua_shared() {
    use std::path::PathBuf;

    let out_dir = std::env::var("OUT_DIR").unwrap();

    // Navigate from OUT_DIR to the target profile directory
    // OUT_DIR is typically: <workspace>/target/<profile>/build/<crate>-<hash>/out
    let target_dir = PathBuf::from(&out_dir)
        .ancestors()
        .nth(3)
        .expect("Could not determine target directory from OUT_DIR")
        .to_path_buf();

    let dll_dest = target_dir.join("lua54.dll");

    // Skip rebuild if lua54.dll already exists in target dir
    if dll_dest.exists() {
        println!("cargo:rerun-if-changed=build.rs");
        return;
    }

    // Find Lua source in cargo registry
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .expect("Could not find home directory");

    let registry_src = PathBuf::from(&home).join(".cargo/registry/src");

    let mut lua_src_dir = None;
    if let Ok(entries) = std::fs::read_dir(&registry_src) {
        for entry in entries.flatten() {
            let index_dir = entry.path();
            if let Ok(packages) = std::fs::read_dir(&index_dir) {
                for pkg in packages.flatten() {
                    let name = pkg.file_name().to_string_lossy().to_string();
                    if name.starts_with("lua-src-547") {
                        let lua_dir = pkg.path().join("lua-5.4.7");
                        if lua_dir.exists() {
                            lua_src_dir = Some(lua_dir);
                            break;
                        }
                    }
                }
            }
            if lua_src_dir.is_some() {
                break;
            }
        }
    }

    let lua_src = match lua_src_dir {
        Some(dir) => dir,
        None => {
            println!("cargo:warning=Could not find lua-src in cargo registry, skipping lua54.dll build");
            println!("cargo:warning=Native module support will not be available");
            return;
        }
    };

    let lua_c_files: Vec<&str> = vec![
        "lapi.c", "lauxlib.c", "lbaselib.c", "lcode.c", "lcorolib.c",
        "lctype.c", "ldblib.c", "ldebug.c", "ldo.c", "ldump.c",
        "lfunc.c", "lgc.c", "linit.c", "liolib.c", "llex.c",
        "lmathlib.c", "lmem.c", "loadlib.c", "lobject.c", "lopcodes.c",
        "loslib.c", "lparser.c", "lstate.c", "lstring.c", "lstrlib.c",
        "ltable.c", "ltablib.c", "ltm.c", "lundump.c", "lutf8lib.c",
        "lvm.c", "lzio.c",
    ];

    // Use cc to find the MSVC compiler
    let compiler = cc::Build::new()
        .define("LUA_BUILD_AS_DLL", None)
        .include(&lua_src)
        .opt_level(2)
        .warnings(false)
        .get_compiler();

    let obj_dir = PathBuf::from(&out_dir).join("lua_obj");
    std::fs::create_dir_all(&obj_dir).unwrap();

    let mut obj_files = Vec::new();

    for c_file in &lua_c_files {
        let src = lua_src.join(c_file);
        let obj = obj_dir.join(c_file.replace(".c", ".obj"));

        let mut cmd = compiler.to_command();
        cmd.arg("/c")
            .arg("/DLUA_BUILD_AS_DLL")
            .arg(format!("/I{}", lua_src.display()))
            .arg(format!("/Fo{}", obj.display()))
            .arg(&src);

        let status = cmd.status().expect("Failed to run compiler");
        if !status.success() {
            println!("cargo:warning=Failed to compile {}, skipping lua54.dll build", c_file);
            return;
        }

        obj_files.push(obj);
    }

    // Find MSVC link.exe with correct environment
    let linker_tool = match cc::windows_registry::find_tool("x86_64-pc-windows-msvc", "link.exe") {
        Some(tool) => tool,
        None => {
            println!("cargo:warning=Could not find MSVC link.exe, skipping lua54.dll build");
            return;
        }
    };

    let dll_path = PathBuf::from(&out_dir).join("lua54.dll");

    let mut link_cmd = linker_tool.to_command();
    link_cmd
        .arg("/nologo")
        .arg("/DLL")
        .arg(format!("/OUT:{}", dll_path.display()));

    for obj in &obj_files {
        link_cmd.arg(obj);
    }

    let status = link_cmd.status().expect("Failed to run linker");
    if !status.success() {
        println!("cargo:warning=Failed to link lua54.dll");
        return;
    }

    // Copy lua54.dll to the target directory (next to the binary)
    if let Err(e) = std::fs::copy(&dll_path, &dll_dest) {
        println!("cargo:warning=Failed to copy lua54.dll to target dir: {}", e);
    }

    // Also copy the import library
    let lib_path = PathBuf::from(&out_dir).join("lua54.lib");
    if lib_path.exists() {
        let _ = std::fs::copy(&lib_path, target_dir.join("lua54.lib"));
    }

    println!("cargo:rerun-if-changed=build.rs");
}
