//! Crypto module for CopperMoon
//!
//! Provides cryptographic utilities.

use coppermoon_core::Result;
use mlua::{Lua, Table};

/// Register the crypto module
pub fn register(lua: &Lua) -> Result<Table> {
    let crypto_table = lua.create_table()?;

    // crypto.sha256(data) -> string
    crypto_table.set("sha256", lua.create_function(crypto_sha256)?)?;

    // crypto.sha1(data) -> string
    crypto_table.set("sha1", lua.create_function(crypto_sha1)?)?;

    // crypto.md5(data) -> string
    crypto_table.set("md5", lua.create_function(crypto_md5)?)?;

    // crypto.hmac(algo, key, data) -> string
    crypto_table.set("hmac", lua.create_function(crypto_hmac)?)?;

    // crypto.random_bytes(n) -> string
    crypto_table.set("random_bytes", lua.create_function(crypto_random_bytes)?)?;

    // crypto.uuid() -> string
    crypto_table.set("uuid", lua.create_function(crypto_uuid)?)?;

    // crypto.base64_encode(data) -> string
    crypto_table.set("base64_encode", lua.create_function(crypto_base64_encode)?)?;

    // crypto.base64_decode(data) -> string
    crypto_table.set("base64_decode", lua.create_function(crypto_base64_decode)?)?;

    // crypto.hex_encode(data) -> string
    crypto_table.set("hex_encode", lua.create_function(crypto_hex_encode)?)?;

    // crypto.hex_decode(data) -> string
    crypto_table.set("hex_decode", lua.create_function(crypto_hex_decode)?)?;

    Ok(crypto_table)
}

fn crypto_sha256(_: &Lua, data: mlua::String) -> mlua::Result<String> {
    use sha2::{Sha256, Digest};

    let bytes = data.as_bytes();
    let mut hasher = Sha256::new();
    hasher.update(&*bytes);
    let result = hasher.finalize();

    Ok(hex::encode(result))
}

fn crypto_sha1(_: &Lua, data: mlua::String) -> mlua::Result<String> {
    use sha1::{Sha1, Digest};

    let bytes = data.as_bytes();
    let mut hasher = Sha1::new();
    hasher.update(&*bytes);
    let result = hasher.finalize();

    Ok(hex::encode(result))
}

fn crypto_md5(_: &Lua, data: mlua::String) -> mlua::Result<String> {
    let bytes = data.as_bytes();
    let digest = md5::compute(&*bytes);

    Ok(format!("{:x}", digest))
}

fn crypto_hmac(_: &Lua, (algo, key, data): (String, mlua::String, mlua::String)) -> mlua::Result<String> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    use sha1::Sha1;

    let key_bytes: Vec<u8> = key.as_bytes().to_vec();
    let data_bytes: Vec<u8> = data.as_bytes().to_vec();

    match algo.to_lowercase().as_str() {
        "sha256" => {
            type HmacSha256 = Hmac<Sha256>;
            let mut mac = HmacSha256::new_from_slice(&key_bytes)
                .map_err(|e| mlua::Error::runtime(format!("HMAC key error: {}", e)))?;
            mac.update(&data_bytes);
            let result = mac.finalize();
            Ok(hex::encode(result.into_bytes()))
        }
        "sha1" => {
            type HmacSha1 = Hmac<Sha1>;
            let mut mac = HmacSha1::new_from_slice(&key_bytes)
                .map_err(|e| mlua::Error::runtime(format!("HMAC key error: {}", e)))?;
            mac.update(&data_bytes);
            let result = mac.finalize();
            Ok(hex::encode(result.into_bytes()))
        }
        _ => Err(mlua::Error::runtime(format!(
            "Unsupported HMAC algorithm: {}. Use 'sha256' or 'sha1'",
            algo
        ))),
    }
}

fn crypto_random_bytes(lua: &Lua, n: usize) -> mlua::Result<mlua::String> {
    use rand::RngCore;

    let mut bytes = vec![0u8; n];
    rand::rng().fill_bytes(&mut bytes);

    lua.create_string(&bytes)
}

fn crypto_uuid(_: &Lua, _: ()) -> mlua::Result<String> {
    Ok(uuid::Uuid::new_v4().to_string())
}

fn crypto_base64_encode(_: &Lua, data: mlua::String) -> mlua::Result<String> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let bytes: Vec<u8> = data.as_bytes().to_vec();
    Ok(STANDARD.encode(&bytes))
}

fn crypto_base64_decode(lua: &Lua, data: String) -> mlua::Result<mlua::String> {
    use base64::{Engine, engine::general_purpose::STANDARD};

    let bytes = STANDARD.decode(&data)
        .map_err(|e| mlua::Error::runtime(format!("Base64 decode error: {}", e)))?;

    lua.create_string(&bytes)
}

fn crypto_hex_encode(_: &Lua, data: mlua::String) -> mlua::Result<String> {
    let bytes: Vec<u8> = data.as_bytes().to_vec();
    Ok(hex::encode(&bytes))
}

fn crypto_hex_decode(lua: &Lua, data: String) -> mlua::Result<mlua::String> {
    let bytes = hex::decode(&data)
        .map_err(|e| mlua::Error::runtime(format!("Hex decode error: {}", e)))?;

    lua.create_string(&bytes)
}
