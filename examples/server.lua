-- Simple HTTP Server example for CopperMoon

local server = http.server.new()

-- Home page
server:get("/", function(ctx)
    return ctx:html([[
<!DOCTYPE html>
<html>
<head>
    <title>CopperMoon Server</title>
    <style>
        body { font-family: sans-serif; max-width: 800px; margin: 50px auto; padding: 20px; }
        h1 { color: #d97706; }
        pre { background: #f3f4f6; padding: 15px; border-radius: 5px; }
        a { color: #2563eb; }
    </style>
</head>
<body>
    <h1>Welcome to CopperMoon!</h1>
    <p>A high-performance Lua runtime written in Rust.</p>

    <h2>Try these endpoints:</h2>
    <ul>
        <li><a href="/api/hello">/api/hello</a> - JSON response</li>
        <li><a href="/api/time">/api/time</a> - Current time</li>
        <li><a href="/api/echo?message=Hello">/api/echo?message=Hello</a> - Echo query params</li>
    </ul>
</body>
</html>
    ]])
end)

-- JSON API endpoint
server:get("/api/hello", function(ctx)
    return ctx:json({
        message = "Hello from CopperMoon!",
        version = _COPPERMOON_VERSION,
        platform = os_ext.platform()
    })
end)

-- Time endpoint
server:get("/api/time", function(ctx)
    return ctx:json({
        timestamp = time.now(),
        timestamp_ms = time.now_ms(),
        uptime = time.monotonic()
    })
end)

-- Echo endpoint with query params
server:get("/api/echo", function(ctx)
    return ctx:json({
        method = ctx.method,
        path = ctx.path,
        query = ctx.query,
        headers = ctx.headers
    })
end)

-- POST endpoint
server:post("/api/data", function(ctx)
    local body = ctx.body
    local data = nil

    -- Try to parse JSON body
    if body and #body > 0 then
        local ok, parsed = pcall(json.decode, body)
        if ok then
            data = parsed
        end
    end

    return ctx:json({
        received = true,
        body_length = #(body or ""),
        data = data
    })
end)

-- 404 for undefined routes is automatic

-- Start the server
local port = 3000
server:listen(port, function(p)
    print("Server starting on port " .. p)
    print("Open http://localhost:" .. p .. " in your browser")
end)
