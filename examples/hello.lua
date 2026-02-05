-- Hello World example for CopperMoon

print("Hello from CopperMoon!")
print("Version:", _COPPERMOON_VERSION)

-- Test basic Lua features
local x = 10
local y = 20
print("x + y =", x + y)

-- Test tables
local user = {
    name = "Alice",
    age = 30
}
print("User:", user)

-- Test functions
local function greet(name)
    return "Hello, " .. name .. "!"
end

print(greet("World"))

-- Test command line arguments
if arg then
    print("Script:", arg[0])
    for i = 1, #arg do
        print("Arg " .. i .. ":", arg[i])
    end
end
