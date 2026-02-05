use mlua::prelude::*;

#[mlua::lua_module]
fn hello_native(lua: &Lua) -> LuaResult<LuaTable> {
    let module = lua.create_table()?;

    module.set("greeting", "Hello from a native Rust module!")?;
    module.set("version", "0.1.0")?;

    let add = lua.create_function(|_, (a, b): (f64, f64)| Ok(a + b))?;
    module.set("add", add)?;

    let greet = lua.create_function(|_, name: String| {
        Ok(format!("Hello, {}! (from Rust)", name))
    })?;
    module.set("greet", greet)?;

    Ok(module)
}
