-- Freight ORM Example
-- Demonstrates the usage of Freight ORM with SQLite
--
-- Run with: coppermoon examples/freight_example.lua

local freight = require("freight")

print("=== Freight ORM Example ===\n")

--------------------------------------------------------------------------------
-- Connect to Database
--------------------------------------------------------------------------------

print("Connecting to SQLite database...")
local db = freight.open("sqlite", {
    database = ":memory:"  -- Use in-memory database for demo
})
print("Connected!\n")

--------------------------------------------------------------------------------
-- Define Models
--------------------------------------------------------------------------------

print("Defining models...")

-- User model
local User = db:model("users", {
    id = freight.primaryKey(),
    name = freight.string(100, { notNull = true }),
    email = freight.string(255, { unique = true }),
    age = freight.integer({ default = 0 }),
    active = freight.boolean({ default = true }),
    created_at = freight.datetime({ default = "CURRENT_TIMESTAMP" })
})

-- Post model
local Post = db:model("posts", {
    id = freight.primaryKey(),
    user_id = freight.foreignKey("users"),
    title = freight.string(200, { notNull = true }),
    content = freight.text(),
    views = freight.integer({ default = 0 }),
    published = freight.boolean({ default = false }),
    created_at = freight.datetime({ default = "CURRENT_TIMESTAMP" })
})

-- Comment model
local Comment = db:model("comments", {
    id = freight.primaryKey(),
    post_id = freight.foreignKey("posts"),
    user_id = freight.foreignKey("users"),
    content = freight.text({ notNull = true }),
    created_at = freight.datetime({ default = "CURRENT_TIMESTAMP" })
})

-- Define relations
User:hasMany(Post, { foreignKey = "user_id" })
Post:belongsTo(User, { foreignKey = "user_id" })
Post:hasMany(Comment, { foreignKey = "post_id" })
Comment:belongsTo(Post, { foreignKey = "post_id" })
Comment:belongsTo(User, { foreignKey = "user_id" })

print("Models defined!\n")

--------------------------------------------------------------------------------
-- Auto Migration
--------------------------------------------------------------------------------

print("Running migrations...")
db:autoMigrate(User, Post, Comment)
print("Migrations complete!\n")

--------------------------------------------------------------------------------
-- Add Hooks
--------------------------------------------------------------------------------

User:beforeCreate(function(data)
    print("  [Hook] Creating user: " .. (data.name or "unknown"))
end)

User:afterCreate(function(record)
    print("  [Hook] User created with ID: " .. record.id)
end)

--------------------------------------------------------------------------------
-- Create Records
--------------------------------------------------------------------------------

print("Creating users...")

local alice = User:create({
    name = "Alice Johnson",
    email = "alice@example.com",
    age = 28
})

local bob = User:create({
    name = "Bob Smith",
    email = "bob@example.com",
    age = 35
})

local charlie = User:create({
    name = "Charlie Brown",
    email = "charlie@example.com",
    age = 22,
    active = false
})

print("")

--------------------------------------------------------------------------------
-- Create Posts
--------------------------------------------------------------------------------

print("Creating posts...")

local post1 = Post:create({
    user_id = alice.id,
    title = "Getting Started with CopperMoon",
    content = "CopperMoon is an amazing Lua runtime...",
    published = true,
    views = 100
})

local post2 = Post:create({
    user_id = alice.id,
    title = "Advanced Lua Techniques",
    content = "Let's explore some advanced patterns...",
    published = true,
    views = 50
})

local post3 = Post:create({
    user_id = bob.id,
    title = "Draft: My Thoughts",
    content = "This is a draft post...",
    published = false
})

print("Posts created!\n")

--------------------------------------------------------------------------------
-- Create Comments
--------------------------------------------------------------------------------

print("Creating comments...")

Comment:create({
    post_id = post1.id,
    user_id = bob.id,
    content = "Great article! Very helpful."
})

Comment:create({
    post_id = post1.id,
    user_id = charlie.id,
    content = "Thanks for sharing!"
})

Comment:create({
    post_id = post2.id,
    user_id = bob.id,
    content = "Looking forward to more content."
})

print("Comments created!\n")

--------------------------------------------------------------------------------
-- Query Examples
--------------------------------------------------------------------------------

print("=== Query Examples ===\n")

-- Find all users
print("All users:")
local users = User:findAll()
for _, user in ipairs(users) do
    print(string.format("  - %s (%s), age %d, active: %s",
        user.name, user.email, user.age, tostring(user.active)))
end
print("")

-- Find user by ID
print("Find user by ID (1):")
local user = User:find(1)
if user then
    print(string.format("  Found: %s", user.name))
end
print("")

-- Where clause with conditions
print("Active users older than 25:")
local adults = User:where("age > ?", 25):where({ active = true }):findAll()
for _, u in ipairs(adults) do
    print(string.format("  - %s (age %d)", u.name, u.age))
end
print("")

-- Order and limit
print("Top 2 users by age (descending):")
local oldest = User:orderBy("age", "DESC"):limit(2):findAll()
for _, u in ipairs(oldest) do
    print(string.format("  - %s (age %d)", u.name, u.age))
end
print("")

-- Count
print("User count: " .. User:count())
print("Active users: " .. User:where({ active = true }):count())
print("")

-- Aggregations
print("Average age: " .. User:avg("age"))
print("Max age: " .. User:max("age"))
print("Min age: " .. User:min("age"))
print("")

-- Published posts with views > 30
print("Popular published posts (views > 30):")
local popular = Post:where("published = ?", true)
    :where("views > ?", 30)
    :orderBy("views", "DESC")
    :findAll()
for _, p in ipairs(popular) do
    print(string.format("  - '%s' (%d views)", p.title, p.views))
end
print("")

--------------------------------------------------------------------------------
-- Update Records
--------------------------------------------------------------------------------

print("=== Update Examples ===\n")

-- Update single record
print("Updating Alice's age...")
User:where("id = ?", alice.id):update({ age = 29 })

local updated_alice = User:find(alice.id)
print(string.format("  Alice's new age: %d", updated_alice.age))
print("")

-- Bulk update
print("Incrementing all post views...")
Post:where("published = ?", true):update({ views = 150 })
print("")

--------------------------------------------------------------------------------
-- Relations
--------------------------------------------------------------------------------

print("=== Relations ===\n")

-- Get user's posts
print("Alice's posts:")
local alice_posts = alice:getPosts()
for _, p in ipairs(alice_posts) do
    print(string.format("  - %s", p.title))
end
print("")

-- Get post's author
print("Post 1 author:")
local author = post1:getUser()
if author then
    print(string.format("  Written by: %s", author.name))
end
print("")

-- Get post's comments
print("Comments on post 1:")
local comments = post1:getComments()
for _, c in ipairs(comments) do
    print(string.format("  - %s", c.content))
end
print("")

--------------------------------------------------------------------------------
-- Transactions
--------------------------------------------------------------------------------

print("=== Transactions ===\n")

print("Running transaction...")
local success = db:transaction(function()
    User:create({ name = "Transaction User", email = "tx@example.com" })
    Post:create({
        user_id = 4,
        title = "Transaction Post",
        content = "Created in transaction"
    })
end)
print("Transaction " .. (success and "committed" or "rolled back"))
print("User count after transaction: " .. User:count())
print("")

--------------------------------------------------------------------------------
-- Delete Records
--------------------------------------------------------------------------------

print("=== Delete Examples ===\n")

print("Deleting unpublished posts...")
local deleted = Post:where("published = ?", false):delete()
print(string.format("  Deleted %d posts", deleted))
print("Post count: " .. Post:count())
print("")

--------------------------------------------------------------------------------
-- Raw Queries
--------------------------------------------------------------------------------

print("=== Raw Query ===\n")

local results = db:raw([[
    SELECT u.name, COUNT(p.id) as post_count
    FROM users u
    LEFT JOIN posts p ON p.user_id = u.id
    GROUP BY u.id
    ORDER BY post_count DESC
]])

print("Users with post counts:")
for _, row in ipairs(results) do
    print(string.format("  - %s: %d posts", row.name, row.post_count or 0))
end
print("")

--------------------------------------------------------------------------------
-- First or Create
--------------------------------------------------------------------------------

print("=== First or Create ===\n")

local dave, created = User:firstOrCreate(
    { email = "dave@example.com" },
    { name = "Dave Wilson", age = 40 }
)
print(string.format("User: %s, Created: %s", dave.name, tostring(created)))

-- Second call should find existing
local dave2, created2 = User:firstOrCreate(
    { email = "dave@example.com" },
    { name = "Different Name", age = 50 }
)
print(string.format("User: %s, Created: %s", dave2.name, tostring(created2)))
print("")

--------------------------------------------------------------------------------
-- Cleanup
--------------------------------------------------------------------------------

print("=== Complete ===")
print("Total users: " .. User:count())
print("Total posts: " .. Post:count())
print("Total comments: " .. Comment:count())

-- Close database
db:close()
print("\nDatabase closed.")
