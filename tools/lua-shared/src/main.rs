use std::path::PathBuf;

fn main() {
    // Set env vars that cc crate expects (normally set by Cargo in build scripts)
    if std::env::var("TARGET").is_err() {
        std::env::set_var("TARGET", "x86_64-pc-windows-msvc");
    }
    if std::env::var("HOST").is_err() {
        std::env::set_var("HOST", "x86_64-pc-windows-msvc");
    }
    if std::env::var("OPT_LEVEL").is_err() {
        std::env::set_var("OPT_LEVEL", "2");
    }

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
            if lua_src_dir.is_some() { break; }
        }
    }

    let lua_src = lua_src_dir.expect("Could not find lua-src in cargo registry");
    println!("Found Lua source at: {}", lua_src.display());

    let out_dir = std::env::current_dir().unwrap();

    let lua_c_files: Vec<&str> = vec![
        "lapi.c", "lauxlib.c", "lbaselib.c", "lcode.c", "lcorolib.c",
        "lctype.c", "ldblib.c", "ldebug.c", "ldo.c", "ldump.c",
        "lfunc.c", "lgc.c", "linit.c", "liolib.c", "llex.c",
        "lmathlib.c", "lmem.c", "loadlib.c", "lobject.c", "lopcodes.c",
        "loslib.c", "lparser.c", "lstate.c", "lstring.c", "lstrlib.c",
        "ltable.c", "ltablib.c", "ltm.c", "lundump.c", "lutf8lib.c",
        "lvm.c", "lzio.c",
    ];

    // Use cc to find the compiler and compile objects
    let compiler = cc::Build::new()
        .define("LUA_BUILD_AS_DLL", None)
        .include(&lua_src)
        .opt_level(2)
        .warnings(false)
        .get_compiler();

    let obj_dir = out_dir.join("lua_obj");
    std::fs::create_dir_all(&obj_dir).unwrap();

    let mut obj_files = Vec::new();

    for c_file in &lua_c_files {
        let src = lua_src.join(c_file);
        let obj = obj_dir.join(c_file.replace(".c", ".obj"));

        println!("Compiling {}", c_file);

        let mut cmd = compiler.to_command();
        cmd.arg("/c")
            .arg("/DLUA_BUILD_AS_DLL")
            .arg(format!("/I{}", lua_src.display()))
            .arg(format!("/Fo{}", obj.display()))
            .arg(&src);

        let status = cmd.status().expect("Failed to run compiler");
        if !status.success() {
            panic!("Failed to compile {}", c_file);
        }

        obj_files.push(obj);
    }

    // Link into DLL
    println!("Linking lua54.dll...");

    let dll_path = out_dir.join("lua54.dll");

    // Use cc::windows_registry to find link.exe with correct environment (LIB paths etc.)
    let linker_tool = cc::windows_registry::find_tool("x86_64-pc-windows-msvc", "link.exe")
        .expect("Could not find MSVC link.exe via cc windows_registry");

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
        panic!("Failed to link lua54.dll");
    }

    println!("\nSuccessfully built: {}", dll_path.display());

    // Also report the .lib file for modules that need it
    let lib_path = out_dir.join("lua54.lib");
    if lib_path.exists() {
        println!("Import library: {}", lib_path.display());
    }
}
