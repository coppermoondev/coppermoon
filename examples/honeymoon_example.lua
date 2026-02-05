-- HoneyMoon Web Framework Example

-- Add packages to path
package.path = package.path .. ";./packages/?/init.lua"

local honeymoon = require("honeymoon")

-- Create application
local app = honeymoon.new()

-- Use built-in middleware
app:use(honeymoon.logger())
app:use(honeymoon.cors())
app:use(honeymoon.json())

-- Serve static files from ./public
-- app:use(honeymoon.static("./public"))

-- Simple routes
app:get("/", function(req, res)
    res:html([[
        <html>
        <head><title>HoneyMoon</title></head>
        <body>
            <h1>Welcome to HoneyMoon!</h1>
            <p>A fast, minimalist web framework for CopperMoon.</p>
            <ul>
                <li><a href="/api/hello">Hello API</a></li>
                <li><a href="/api/users">Users API</a></li>
                <li><a href="/api/users/1">User 1</a></li>
            </ul>
        </body>
        </html>
    ]])
end)

app:get("/api/hello", function(req, res)
    res:json({
        message = "Hello from HoneyMoon!",
        timestamp = time.now()
    })
end)

-- Route with parameters
app:get("/api/users/:id", function(req, res)
    local user_id = req.params.id

    res:json({
        id = tonumber(user_id),
        name = "User " .. user_id,
        email = "user" .. user_id .. "@example.com"
    })
end)

-- Multiple handlers (middleware chain)
local function auth_middleware(req, res, next)
    local token = req:get("authorization")
    if not token then
        -- For demo, allow access anyway
        req.user = { id = 0, role = "guest" }
    else
        req.user = { id = 1, role = "admin" }
    end
    next()
end

app:get("/api/protected", auth_middleware, function(req, res)
    res:json({
        message = "Protected resource",
        user = req.user
    })
end)

-- List of users
local users = {
    { id = 1, name = "Alice", email = "alice@example.com" },
    { id = 2, name = "Bob", email = "bob@example.com" },
    { id = 3, name = "Charlie", email = "charlie@example.com" },
}

app:get("/api/users", function(req, res)
    res:json(users)
end)

-- Create user (POST)
app:post("/api/users", function(req, res)
    local body = req:json()

    local new_user = {
        id = #users + 1,
        name = body.name or "Unknown",
        email = body.email or ""
    }

    table.insert(users, new_user)

    res:status(201):json(new_user)
end)

-- Error handling
app:error(function(err, req, res)
    print("Error occurred:", err)
    res:status(500):json({
        error = "Internal Server Error",
        message = tostring(err)
    })
end)

-- Create a router for /admin routes
local admin = app:router()

admin:get("/", function(req, res)
    res:json({ message = "Admin dashboard" })
end)

admin:get("/stats", function(req, res)
    res:json({
        total_users = #users,
        uptime = time.monotonic()
    })
end)

-- Mount router
app:mount("/admin", admin)

-- 404 handler (catch-all at the end)
app:get("*", function(req, res)
    res:status(404):json({
        error = "Not Found",
        path = req.path
    })
end)

-- Start server
app:listen(3000, function(port)
    print("HoneyMoon example server running!")
    print("Visit http://127.0.0.1:" .. port)
end)
