-- Test CopperMoon Standard Library

print("=== Testing CopperMoon Standard Library ===")
print()

-- Test fs module
print("--- fs module ---")
local test_file = "test_file.txt"
fs.write(test_file, "Hello from CopperMoon!")
print("Written file:", test_file)

local content = fs.read(test_file)
print("Read content:", content)

print("File exists:", fs.exists(test_file))
print("Is file:", fs.is_file(test_file))
print("Is dir:", fs.is_dir(test_file))

local stat = fs.stat(test_file)
print("File size:", stat.size)

fs.remove(test_file)
print("File removed, exists:", fs.exists(test_file))
print()

-- Test path module
print("--- path module ---")
print("Path separator:", path.sep)
print("Join paths:", path.join("foo", "bar", "baz.txt"))
print("Dirname:", path.dirname("/home/user/file.txt"))
print("Basename:", path.basename("/home/user/file.txt"))
print("Extname:", path.extname("document.pdf"))
print("Is absolute /home:", path.is_absolute("/home"))
print("Is relative ./foo:", path.is_relative("./foo"))
print()

-- Test os_ext module
print("--- os_ext module ---")
print("Platform:", os_ext.platform())
print("Architecture:", os_ext.arch())
print("Current dir:", os_ext.cwd())
print("Home dir:", os_ext.homedir())
print("Temp dir:", os_ext.tmpdir())
print("CPU cores:", os_ext.cpus())

os_ext.setenv("COPPERMOON_TEST", "hello")
print("Env var:", os_ext.env("COPPERMOON_TEST"))
os_ext.unsetenv("COPPERMOON_TEST")
print()

-- Test process module
print("--- process module ---")
print("PID:", process.pid())

local result
if os_ext.platform() == "windows" then
    result = process.exec("echo Hello from shell")
else
    result = process.exec("echo 'Hello from shell'")
end
print("Exec result:", result.stdout:gsub("\n", ""))
print()

-- Test json module
print("--- json module ---")
local data = {
    name = "Alice",
    age = 30,
    hobbies = {"reading", "coding", "music"}
}

local json_str = json.encode(data)
print("Encoded:", json_str)

local decoded = json.decode(json_str)
print("Decoded name:", decoded.name)
print("Decoded age:", decoded.age)

print("Pretty JSON:")
print(json.pretty({hello = "world", numbers = {1, 2, 3}}))
print()

-- Test crypto module
print("--- crypto module ---")
print("UUID:", crypto.uuid())
print("SHA256 of 'hello':", crypto.sha256("hello"))
print("MD5 of 'hello':", crypto.md5("hello"))

local encoded = crypto.base64_encode("Hello, World!")
print("Base64 encoded:", encoded)
print("Base64 decoded:", crypto.base64_decode(encoded))

print("Hex encoded 'ABC':", crypto.hex_encode("ABC"))
print("Hex decoded '414243':", crypto.hex_decode("414243"))
print()

print("=== All tests passed! ===")
