-- FlappyMoon — un Flappy Bird propulsé par Andesite + CopperMoon.
--
-- Toolkit exercé :
--   · app:on_frame(dt, ui) + painter (rect/text/image)  → rendu du jeu
--   · andesite.image + uv (spritesheet) + rotation      → oiseau animé
--   · andesite.audio                                     → sons générés en Lua
--   · andesite.input (clavier + souris)                  → contrôles
--   · andesite.storage                                   → meilleur score persistant
--   · andesite.notify                                    → toast sur nouveau record
--   · andesite.clipboard                                 → partager son score
--   · require + modules lib/                             → structure du projet
--
-- Contrôles : ESPACE ou clic gauche pour battre des ailes.

local assets = require("lib.assets")
local game = require("lib.game")

local app = andesite.app()

-- Assets générés au premier lancement (voir lib/assets.lua)
local paths = assets.ensure()
local bird_img = andesite.image.load(paths.bird)

-- Sons : jamais fatals (machine sans audio = jeu silencieux)
local function play(name, volume)
    pcall(function()
        andesite.audio.play(paths[name], { volume = volume or 1.0 })
    end)
end

-- ---------------------------------------------------------------------------
-- État global
-- ---------------------------------------------------------------------------

local MODE = "menu" -- "menu" | "play" | "dead"
local g = nil       -- état de partie (lib/game.lua)
local best = andesite.storage.get("best", 0)
local new_record = false
local copied = false
local wing_timer, wing_frame = 0, 0
local idle_time = 0

-- Fenêtre de tuning ImGui-style (touche T) : les drag_values sont branchés
-- en direct sur game.tuning — réglez la gravité en pleine partie !
local tuning_open = false
local godmode = false
local prev_t = false
local fps_hist = {}
local frame_i = 0

-- Détection de "pression" (front montant) : key_down/mouse sont des états.
-- Le clic est ignoré quand l'UI capture le pointeur (fenêtre de tuning).
local prev_pressed = false
local function flap_pressed(ui)
    local mouse_ok = not ui:wants_pointer()
    local now = andesite.input.key_down("space")
        or (mouse_ok and andesite.input.mouse().down)
    local pressed = now and not prev_pressed
    prev_pressed = now
    return pressed
end

local function start_game(w, h)
    g = game.new(w, h)
    MODE = "play"
    new_record = false
    copied = false
    play("flap", 0.7)
end

local function die()
    MODE = "dead"
    play("hit")
    if g.score > best then
        best = g.score
        andesite.storage.set("best", best)
        new_record = true
        pcall(function()
            andesite.notify.send("FlappyMoon", "Nouveau record : " .. best .. " points !")
        end)
    end
end

-- ---------------------------------------------------------------------------
-- Rendu (painter) — appelé à chaque frame, ~60 fps
-- ---------------------------------------------------------------------------

local SKY = "#4ec0ca"
local PIPE = "#73bf2e"
local PIPE_DARK = "#557e1f"
local GROUND = "#ded895"
local GROUND_TOP = "#7ec544"

local function draw_pipes(ui, W, H)
    for _, p in ipairs(g.pipes) do
        local pw = game.PIPE_WIDTH
        local bottom_y = p.gap_y + p.gap_h
        -- fûts
        ui:painter_rect(p.x, 0, pw, p.gap_y, PIPE)
        ui:painter_rect(p.x, bottom_y, pw, (H - game.GROUND_H) - bottom_y, PIPE)
        -- collerettes + liserés sombres
        ui:painter_rect(p.x - 3, p.gap_y - 22, pw + 6, 22, PIPE)
        ui:painter_rect(p.x - 3, bottom_y, pw + 6, 22, PIPE)
        ui:painter_rect(p.x - 3, p.gap_y - 22, 4, 22, PIPE_DARK)
        ui:painter_rect(p.x - 3, bottom_y, 4, 22, PIPE_DARK)
        ui:painter_rect(p.x, 0, 4, p.gap_y - 22, PIPE_DARK)
        ui:painter_rect(p.x, bottom_y + 22, 4, (H - game.GROUND_H) - bottom_y - 22, PIPE_DARK)
    end
end

local function draw_ground(ui, W, H, scroll)
    local y = H - game.GROUND_H
    ui:painter_rect(0, y, W, game.GROUND_H, GROUND)
    ui:painter_rect(0, y, W, 12, GROUND_TOP)
    -- hachures défilantes
    local x = -scroll
    while x < W do
        ui:painter_line(x, y + 2, x + 12, y + 10, 3, "#6aa93a")
        x = x + 24
    end
end

local function draw_bird(ui, x, y, angle)
    local uv = wing_frame == 0 and { 0, 0, 0.5, 1 } or { 0.5, 0, 1, 1 }
    ui:painter_image(bird_img, x - 24, y - 24, 48, 48, { uv = uv, rotation = angle })
end

local function draw_score(ui, W)
    local text = tostring(g.score)
    ui:painter_text(W / 2 - #text * 12, 28, text, 44, "#ffffff")
end

app:on_frame(function(dt, ui)
    local W, H = ui:available_size()

    -- Historique fps (avant le clamp) + toggle de la fenêtre de tuning
    frame_i = frame_i + 1
    fps_hist[#fps_hist + 1] = { frame_i, 1 / math.max(dt, 0.0001) }
    if #fps_hist > 150 then table.remove(fps_hist, 1) end
    local t_now = andesite.input.key_down("t")
    if t_now and not prev_t then tuning_open = not tuning_open end
    prev_t = t_now

    if dt > 0.033 then dt = 0.033 end

    -- battement d'ailes : rapide en vol, lent dans les menus
    wing_timer = wing_timer + dt
    local wing_speed = (MODE == "play") and 0.09 or 0.22
    if wing_timer >= wing_speed then
        wing_timer = 0
        wing_frame = 1 - wing_frame
    end

    ui:painter_rect(0, 0, W, H, SKY)

    if MODE == "menu" then
        idle_time = idle_time + dt
        draw_ground(ui, W, H, (idle_time * 60) % 24)
        -- l'oiseau flotte doucement
        draw_bird(ui, W * 0.3, H * 0.42 + math.sin(idle_time * 3) * 9, 0)
        if flap_pressed(ui) then
            start_game(W, H)
        end

    elseif MODE == "play" then
        if flap_pressed(ui) then
            game.flap(g)
            play("flap", 0.6)
        end
        local events = game.update(g, dt, W, H)
        if events.scored then play("score", 0.5) end

        draw_pipes(ui, W, H)
        draw_ground(ui, W, H, g.scroll)
        draw_bird(ui, g.bird.x, g.bird.y, game.bird_angle(g))
        draw_score(ui, W)

        if events.died and not godmode then die() end

    else -- "dead" : scène figée
        draw_pipes(ui, W, H)
        draw_ground(ui, W, H, g.scroll)
        draw_bird(ui, g.bird.x, g.bird.y, 1.25)
        if flap_pressed(ui) then
            start_game(W, H)
        end
    end
end)

-- ---------------------------------------------------------------------------
-- Overlays UI (widgets egui) — menus par-dessus la scène
-- ---------------------------------------------------------------------------

app:on_ui(function(ui)
    local W, H = ui:available_size()

    if MODE == "menu" then
        ui:space(H * 0.12)
        ui:vertical(function(v)
            v:space(0)
        end)
        ui:horizontal(function(h)
            h:space(W / 2 - 92)
            h:heading("🌙  FlappyMoon")
        end)
        ui:space(8)
        ui:horizontal(function(h)
            h:space(W / 2 - 110)
            h:label("ESPACE ou clic pour voler — record : " .. best)
        end)
        ui:horizontal(function(h)
            h:space(W / 2 - 82)
            h:colored_label("#e8f7f9aa", "T : fenêtre de tuning (live !)")
        end)
        ui:space(H * 0.42)
        ui:horizontal(function(h)
            h:space(W / 2 - 40)
            if h:button("  Jouer  ") then
                start_game(W, H)
            end
        end)

    elseif MODE == "dead" then
        ui:space(H * 0.18)
        ui:horizontal(function(h)
            h:space(W / 2 - 60)
            h:heading("Game over")
        end)
        ui:space(6)
        ui:horizontal(function(h)
            h:space(W / 2 - 105)
            local msg = "Score : " .. g.score .. "   —   Record : " .. best
            if new_record then
                h:colored_label("#ffd700", msg .. "  ★ nouveau record !")
            else
                h:label(msg)
            end
        end)
        ui:space(14)
        ui:horizontal(function(h)
            h:space(W / 2 - 118)
            if h:button("  Rejouer (espace)  ") then
                start_game(W, H)
            end
            if h:button(copied and "Copié !" or "Copier le score") then
                pcall(function()
                    andesite.clipboard.set_text(
                        "J'ai marqué " .. g.score .. " points sur FlappyMoon ! (record : " .. best .. ")")
                end)
                copied = true
            end
        end)
    end

    -- ------------------------------------------------------------------
    -- Fenêtre de tuning (T) — l'expérience Dear ImGui : réglages en
    -- direct, mode invincible, graphe de fps. Disponible dans tous les
    -- modes, y compris en pleine partie.
    -- ------------------------------------------------------------------
    tuning_open = ui:window("Tuning (T)",
        { open = tuning_open, default_x = 8, default_y = 8, default_width = 300 },
        function(w)
            w:grid("tune", function(t)
                t:label("gravité", "px/s² — accélération vers le bas")
                game.tuning.gravity = t:drag_value(game.tuning.gravity,
                    { speed = 10, min = 200, max = 3000 })
                t:end_row()
                t:label("impulsion", "vitesse verticale au flap (négatif = vers le haut)")
                game.tuning.flap_vy = t:drag_value(game.tuning.flap_vy,
                    { speed = 5, min = -800, max = -100 })
                t:end_row()
                t:label("vitesse", "défilement de base, px/s")
                game.tuning.pipe_speed = t:drag_value(game.tuning.pipe_speed,
                    { speed = 2, min = 40, max = 500 })
                t:end_row()
                t:label("espacement", "distance entre tuyaux")
                game.tuning.pipe_spacing = t:drag_value(game.tuning.pipe_spacing,
                    { speed = 2, min = 120, max = 500 })
                t:end_row()
                t:label("passage", "hauteur du trou entre les tuyaux")
                game.tuning.gap_base = t:drag_value(game.tuning.gap_base,
                    { speed = 2, min = 110, max = 320 })
                t:end_row()
                t:label("accélération", "vitesse gagnée par point marqué")
                game.tuning.speed_ramp = t:drag_value(game.tuning.speed_ramp,
                    { speed = 0.1, min = 0, max = 10 })
                t:end_row()
            end)
            w:space(4)
            w:horizontal(function(h)
                godmode = h:checkbox(godmode, "invincible", "les collisions ne tuent plus")
                if h:button("réinitialiser", "revenir aux réglages d'origine") then
                    game.reset_tuning()
                end
            end)
            w:separator()
            w:plot("fps", { height = 80 }, {
                { label = "fps", kind = "line", color = "#2ecc71", points = fps_hist },
            })
            if g then
                w:monospace(string.format("score %d · tuyaux %d · vy %+.0f",
                    g.score, #g.pipes, g.bird.vy))
            end
        end)
end)

app:run()
