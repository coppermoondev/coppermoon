-- Tailwind CSS Integration Example for HoneyMoon
-- Demonstrates how to use the tailwind package

local honeymoon = require("honeymoon")
local tailwind = require("tailwind")

local app = honeymoon.new()

--------------------------------------------------------------------------------
-- Setup Tailwind with CopperMoon preset
--------------------------------------------------------------------------------

-- Option 1: Quick setup with preset
tailwind.setup(app, tailwind.preset("coppermoon"))

-- Option 2: Manual configuration
-- app:use(tailwind.middleware({
--     mode = "cdn",
--     darkMode = "class",
--     theme = {
--         extend = {
--             colors = {
--                 brand = "#c97c3c",
--             },
--         },
--     },
-- }))

--------------------------------------------------------------------------------
-- Setup Vein templating
--------------------------------------------------------------------------------

app.views:use("vein")
app.views:set("views", "./views")

-- Add tailwind head to all templates
app.views:global("tailwind_head", tailwind.head(tailwind.preset("coppermoon")))

-- Add component helpers
local components = require("tailwind.lib.components")
app.views:helper("btn", components.btn)
app.views:helper("card", components.cardClass)
app.views:helper("input", components.inputClass)
app.views:helper("cn", tailwind.cn)

--------------------------------------------------------------------------------
-- Middleware
--------------------------------------------------------------------------------

app:use(honeymoon.logger())

--------------------------------------------------------------------------------
-- Routes
--------------------------------------------------------------------------------

app:get("/", function(req, res)
    res:send([[
<!DOCTYPE html>
<html lang="en" class="dark">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Tailwind + HoneyMoon Example</title>
    ]] .. tailwind.head(tailwind.preset("coppermoon")) .. [[
</head>
<body class="bg-black text-white min-h-screen">
    <div class="max-w-4xl mx-auto px-4 py-16">
        <h1 class="text-4xl font-bold mb-4 bg-gradient-to-r from-white to-copper-500 bg-clip-text text-transparent">
            Tailwind + HoneyMoon
        </h1>
        <p class="text-zinc-400 text-lg mb-8">
            Beautiful, modern UI with TailwindCSS and HoneyMoon
        </p>
        
        <div class="flex gap-4 mb-12">
            <a href="/components" class="]] .. components.btn("primary", "md") .. [[">
                View Components
            </a>
            <a href="/docs" class="]] .. components.btn("outline", "md") .. [[">
                Documentation
            </a>
        </div>
        
        <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            <div class="]] .. components.cardClass("interactive", "md") .. [[">
                <h3 class="text-lg font-medium mb-2">Easy Integration</h3>
                <p class="text-zinc-400 text-sm">One line setup with HoneyMoon apps</p>
            </div>
            
            <div class="]] .. components.cardClass("interactive", "md") .. [[">
                <h3 class="text-lg font-medium mb-2">CDN or Build</h3>
                <p class="text-zinc-400 text-sm">Use Play CDN for dev, compiled for production</p>
            </div>
            
            <div class="]] .. components.cardClass("interactive", "md") .. [[">
                <h3 class="text-lg font-medium mb-2">CopperMoon Theme</h3>
                <p class="text-zinc-400 text-sm">Pre-configured copper color palette</p>
            </div>
        </div>
    </div>
</body>
</html>
]])
end)

app:get("/components", function(req, res)
    res:send([[
<!DOCTYPE html>
<html lang="en" class="dark">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Components - Tailwind + HoneyMoon</title>
    ]] .. tailwind.head(tailwind.preset("coppermoon")) .. [[
</head>
<body class="bg-black text-white min-h-screen">
    <div class="max-w-4xl mx-auto px-4 py-16">
        <a href="/" class="text-zinc-500 hover:text-white mb-8 inline-block">&larr; Back</a>
        
        <h1 class="text-3xl font-bold mb-8">Component Examples</h1>
        
        <!-- Buttons -->
        <section class="mb-12">
            <h2 class="text-xl font-semibold mb-4 text-zinc-300">Buttons</h2>
            <div class="flex flex-wrap gap-3">
                <button class="]] .. components.btn("primary", "md") .. [[">Primary</button>
                <button class="]] .. components.btn("secondary", "md") .. [[">Secondary</button>
                <button class="]] .. components.btn("outline", "md") .. [[">Outline</button>
                <button class="]] .. components.btn("ghost", "md") .. [[">Ghost</button>
                <button class="]] .. components.btn("danger", "md") .. [[">Danger</button>
            </div>
        </section>
        
        <!-- Inputs -->
        <section class="mb-12">
            <h2 class="text-xl font-semibold mb-4 text-zinc-300">Inputs</h2>
            <div class="space-y-4 max-w-md">
                <input type="text" placeholder="Default input" class="]] .. components.inputClass("default", "md") .. [[">
                <input type="text" placeholder="Filled input" class="]] .. components.inputClass("filled", "md") .. [[">
                <input type="text" placeholder="Outline input" class="]] .. components.inputClass("outline", "md") .. [[">
            </div>
        </section>
        
        <!-- Badges -->
        <section class="mb-12">
            <h2 class="text-xl font-semibold mb-4 text-zinc-300">Badges</h2>
            <div class="flex flex-wrap gap-2">
                <span class="]] .. components.badgeClass("default", "md") .. [[">Default</span>
                <span class="]] .. components.badgeClass("primary", "md") .. [[">Primary</span>
                <span class="]] .. components.badgeClass("success", "md") .. [[">Success</span>
                <span class="]] .. components.badgeClass("warning", "md") .. [[">Warning</span>
                <span class="]] .. components.badgeClass("danger", "md") .. [[">Danger</span>
                <span class="]] .. components.badgeClass("info", "md") .. [[">Info</span>
            </div>
        </section>
        
        <!-- Alerts -->
        <section class="mb-12">
            <h2 class="text-xl font-semibold mb-4 text-zinc-300">Alerts</h2>
            <div class="space-y-4">
                <div class="]] .. components.alertClass("info") .. [[">
                    <strong>Info:</strong> This is an informational message.
                </div>
                <div class="]] .. components.alertClass("success") .. [[">
                    <strong>Success:</strong> Operation completed successfully!
                </div>
                <div class="]] .. components.alertClass("warning") .. [[">
                    <strong>Warning:</strong> Please review before proceeding.
                </div>
                <div class="]] .. components.alertClass("danger") .. [[">
                    <strong>Error:</strong> Something went wrong.
                </div>
            </div>
        </section>
        
        <!-- Cards -->
        <section class="mb-12">
            <h2 class="text-xl font-semibold mb-4 text-zinc-300">Cards</h2>
            <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div class="]] .. components.cardClass("default", "md") .. [[">
                    <h3 class="font-medium mb-2">Default Card</h3>
                    <p class="text-zinc-400 text-sm">Basic card with subtle border</p>
                </div>
                <div class="]] .. components.cardClass("elevated", "md") .. [[">
                    <h3 class="font-medium mb-2">Elevated Card</h3>
                    <p class="text-zinc-400 text-sm">Card with shadow elevation</p>
                </div>
                <div class="]] .. components.cardClass("interactive", "md") .. [[">
                    <h3 class="font-medium mb-2">Interactive Card</h3>
                    <p class="text-zinc-400 text-sm">Hover for copper accent</p>
                </div>
                <div class="]] .. components.cardClass("outline", "md") .. [[">
                    <h3 class="font-medium mb-2">Outline Card</h3>
                    <p class="text-zinc-400 text-sm">Transparent with border</p>
                </div>
            </div>
        </section>
    </div>
</body>
</html>
]])
end)

--------------------------------------------------------------------------------
-- Start Server
--------------------------------------------------------------------------------

local port = 3000
print("Tailwind + HoneyMoon example running on http://localhost:" .. port)
app:listen(port)
