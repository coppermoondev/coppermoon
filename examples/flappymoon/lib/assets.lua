-- assets.lua — génération des assets au premier lancement.
--
-- FlappyMoon n'embarque AUCUN fichier binaire : les sons (WAV) et le
-- spritesheet de l'oiseau (BMP) sont fabriqués en pur Lua avec la stdlib
-- CopperMoon (string.pack + fs). C'est aussi une démo :)

local assets = {}

local RATE = 22050 -- Hz, mono, 16 bits

-- ---------------------------------------------------------------------------
-- WAV
-- ---------------------------------------------------------------------------

--- Encapsule des échantillons PCM 16 bits dans un fichier WAV.
local function wav_file(samples)
    local data = table.concat(samples)
    return "RIFF" .. string.pack("<I4", 36 + #data) .. "WAVE"
        .. "fmt " .. string.pack("<I4I2I2I4I4I2I2", 16, 1, 1, RATE, RATE * 2, 2, 16)
        .. "data" .. string.pack("<I4", #data)
        .. data
end

--- Génère un ton avec balayage de fréquence et enveloppe (attaque/déclin).
-- shape : "sine" (défaut), "square" ou "noise"
local function tone(freq_from, freq_to, duration, volume, shape)
    local n = math.floor(RATE * duration)
    local out = {}
    local phase = 0
    for i = 0, n - 1 do
        local t = i / n
        local freq = freq_from + (freq_to - freq_from) * t
        phase = phase + 2 * math.pi * freq / RATE
        local v
        if shape == "noise" then
            v = math.random() * 2 - 1
        elseif shape == "square" then
            v = math.sin(phase) >= 0 and 1 or -1
        else
            v = math.sin(phase)
        end
        -- attaque de 5 ms puis déclin linéaire : évite les "clics"
        local env = math.min(1, i / (RATE * 0.005)) * (1 - t)
        out[#out + 1] = string.pack("<i2", math.floor(v * env * volume * 32767))
    end
    return out
end

local function concat_tones(...)
    local all = {}
    for _, part in ipairs({ ... }) do
        table.move(part, 1, #part, #all + 1, all)
    end
    return all
end

-- ---------------------------------------------------------------------------
-- BMP (24 bits, sans compression — trivial à écrire octet par octet)
-- ---------------------------------------------------------------------------

--- Encode une grille de pixels {r,g,b} (lignes de haut en bas) en BMP.
local function bmp_file(width, height, pixels)
    local pad = string.rep("\0", (4 - (width * 3) % 4) % 4)
    local rows = {}
    for y = height, 1, -1 do -- BMP stocke les lignes de bas en haut
        local row = {}
        for x = 1, width do
            local p = pixels[y][x]
            row[x] = string.char(p[3], p[2], p[1]) -- ordre BGR
        end
        rows[#rows + 1] = table.concat(row) .. pad
    end
    local data = table.concat(rows)
    return "BM" .. string.pack("<I4I2I2I4", 54 + #data, 0, 0, 54)
        .. string.pack("<I4i4i4I2I2I4I4i4i4I4I4",
            40, width, height, 1, 24, 0, #data, 2835, 2835, 0, 0)
        .. data
end

-- ---------------------------------------------------------------------------
-- Sprite de l'oiseau : 2 frames 16x16 côte à côte (aile haute / aile basse).
--
-- BMP n'a pas de canal alpha : le fond du sprite est peint couleur ciel,
-- l'oiseau ne survolant que le ciel (il meurt au premier contact !).
-- ---------------------------------------------------------------------------

local PALETTE = {
    ["."] = { 78, 192, 202 },  -- ciel (fond "transparent")
    ["k"] = { 40, 30, 30 },    -- contour
    ["o"] = { 255, 176, 46 },  -- corps orange
    ["O"] = { 255, 214, 117 }, -- ventre clair
    ["w"] = { 255, 255, 255 }, -- blanc de l'oeil
    ["b"] = { 20, 20, 20 },    -- pupille
    ["r"] = { 231, 84, 39 },   -- bec
    ["W"] = { 250, 240, 212 }, -- aile
}

local FRAME_UP = { -- aile relevée
    "................",
    "................",
    ".....kkkkkk.....",
    "....koooooowk...",
    "...koooooowwbk..",
    "..kWWWkoooowwk..",
    ".kWWWWWkoooook..",
    ".kWWWWWkooookrr.",
    ".kkWWWkoooookrrr",
    "..kkkoooOOookrr.",
    "...koOOOOOOok...",
    "...kooOOOOokk...",
    "....kkooookk....",
    "......kkkk......",
    "................",
    "................",
}

local FRAME_DOWN = { -- aile baissée
    "................",
    "................",
    ".....kkkkkk.....",
    "....koooooowk...",
    "...koooooowwbk..",
    "..koooooooowwk..",
    ".koooooooooook..",
    ".kokWWWkoookrrr.",
    ".kokWWWWkookrrrr",
    "..kokWWWkOokrr..",
    "...koOkkOOOok...",
    "...kooOOOOokk...",
    "....kkooookk....",
    "......kkkk......",
    "................",
    "................",
}

local function frames_to_pixels(frames)
    local height = #frames[1]
    local width = 0
    for _, f in ipairs(frames) do
        width = width + #f[1]
    end
    local pixels = {}
    for y = 1, height do
        pixels[y] = {}
        local x0 = 0
        for _, frame in ipairs(frames) do
            local row = frame[y]
            assert(#row == #frame[1], "ligne de sprite de largeur incoherente")
            for x = 1, #row do
                local color = PALETTE[row:sub(x, x)]
                assert(color, "caractere inconnu dans le sprite: '" .. row:sub(x, x) .. "'")
                pixels[y][x0 + x] = color
            end
            x0 = x0 + #row
        end
    end
    return width, height, pixels
end

-- ---------------------------------------------------------------------------
-- API
-- ---------------------------------------------------------------------------

--- Génère les assets manquants et retourne leurs chemins.
function assets.ensure()
    local paths = {
        bird = "assets/bird.bmp",
        flap = "assets/flap.wav",
        score = "assets/score.wav",
        hit = "assets/hit.wav",
    }

    if not fs.exists(paths.bird) then
        local w, h, pixels = frames_to_pixels({ FRAME_UP, FRAME_DOWN })
        fs.write_bytes(paths.bird, bmp_file(w, h, pixels))
    end
    if not fs.exists(paths.flap) then
        -- petit "whoosh" descendant
        fs.write_bytes(paths.flap, wav_file(tone(520, 180, 0.12, 0.45)))
    end
    if not fs.exists(paths.score) then
        -- double ding montant
        fs.write_bytes(paths.score, wav_file(concat_tones(
            tone(880, 880, 0.07, 0.35),
            tone(1318, 1318, 0.09, 0.35)
        )))
    end
    if not fs.exists(paths.hit) then
        -- choc : bruit + basse carrée
        fs.write_bytes(paths.hit, wav_file(concat_tones(
            tone(0, 0, 0.08, 0.5, "noise"),
            tone(110, 60, 0.18, 0.5, "square")
        )))
    end

    return paths
end

return assets
