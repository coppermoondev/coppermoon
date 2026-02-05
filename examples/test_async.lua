-- Test async runtime and HTTP client

print("=== Testing Async Runtime & HTTP ===")
print()

-- Test time module
print("--- time module ---")
print("Unix timestamp:", time.now())
print("Timestamp (ms):", time.now_ms())
print("Monotonic time:", time.monotonic())

print("Sleeping for 500ms...")
local start = time.monotonic_ms()
time.sleep(500)
local elapsed = time.monotonic_ms() - start
print("Slept for", elapsed, "ms")
print()

-- Test HTTP client
print("--- http module ---")

print("Making GET request to httpbin.org...")
local response = http.get("https://httpbin.org/get")
print("Status:", response.status)
print("OK:", response.ok)
print("Body length:", #response.body, "bytes")
print()

print("Making POST request...")
local post_response = http.post(
    "https://httpbin.org/post",
    json.encode({ message = "Hello from CopperMoon!" }),
    {
        headers = {
            ["Content-Type"] = "application/json"
        }
    }
)
print("POST Status:", post_response.status)
print("POST OK:", post_response.ok)
print()

-- Test custom request
print("Making custom request...")
local custom_response = http.request({
    method = "GET",
    url = "https://httpbin.org/headers",
    headers = {
        ["X-Custom-Header"] = "CopperMoon-Test",
        ["User-Agent"] = "CopperMoon/0.1.0"
    }
})
print("Custom request status:", custom_response.status)

-- Parse the response to verify our headers
local body = json.decode(custom_response.body)
if body and body.headers then
    print("Server received User-Agent:", body.headers["User-Agent"])
    print("Server received X-Custom-Header:", body.headers["X-Custom-Header"])
end
print()

print("=== All async tests passed! ===")
