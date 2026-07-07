-- game.lua — logique pure du jeu (physique, tuyaux, collisions, score).
-- Aucune dépendance au rendu : testable et lisible.

local game = {}

-- Réglages
local GRAVITY = 1150       -- px/s²
local FLAP_VY = -340       -- impulsion de saut, px/s
local PIPE_SPEED = 155     -- vitesse de défilement de base, px/s
local PIPE_SPACING = 235   -- distance horizontale entre tuyaux, px
local PIPE_WIDTH = 68
local GAP_BASE = 175       -- hauteur du passage
local GAP_MIN = 135
local BIRD_RADIUS = 14     -- hitbox (le sprite fait 48 px, hitbox plus douce)
local GROUND_H = 72

game.PIPE_WIDTH = PIPE_WIDTH
game.GROUND_H = GROUND_H
game.BIRD_RADIUS = BIRD_RADIUS

--- Nouvel état de partie pour un terrain w×h.
function game.new(w, h)
    local g = {
        bird = { x = w * 0.3, y = h * 0.42, vy = 0 },
        pipes = {},          -- { x, gap_y, gap_h, scored }
        score = 0,
        dist_to_next = PIPE_SPACING * 0.8,
        scroll = 0,          -- défilement du sol (visuel)
        time = 0,
    }
    return g
end

function game.flap(g)
    g.bird.vy = FLAP_VY
end

--- Vitesse courante : accélère doucement avec le score.
local function speed(g)
    return PIPE_SPEED + math.min(60, g.score * 2.5)
end

local function spawn_pipe(g, w, h)
    local gap_h = math.max(GAP_MIN, GAP_BASE - g.score * 1.5)
    local margin = 60
    local playable = h - GROUND_H
    local gap_y = margin + math.random() * (playable - gap_h - margin * 2)
    g.pipes[#g.pipes + 1] = { x = w + PIPE_WIDTH, gap_y = gap_y, gap_h = gap_h, scored = false }
end

--- Collision cercle (oiseau) / rectangle (tuyau).
local function circle_hits_rect(cx, cy, r, rx, ry, rw, rh)
    local nx = math.max(rx, math.min(cx, rx + rw))
    local ny = math.max(ry, math.min(cy, ry + rh))
    local dx, dy = cx - nx, cy - ny
    return dx * dx + dy * dy < r * r
end

--- Avance la simulation de dt. Retourne { scored = bool, died = bool }.
function game.update(g, dt, w, h)
    local events = { scored = false, died = false }
    local v = speed(g)
    g.time = g.time + dt
    g.scroll = (g.scroll + v * dt) % 24

    -- Oiseau
    g.bird.vy = g.bird.vy + GRAVITY * dt
    g.bird.y = g.bird.y + g.bird.vy * dt
    g.bird.x = w * 0.3

    -- Plafond mou, sol dur
    if g.bird.y < BIRD_RADIUS then
        g.bird.y, g.bird.vy = BIRD_RADIUS, 0
    end
    if g.bird.y > h - GROUND_H - BIRD_RADIUS then
        g.bird.y = h - GROUND_H - BIRD_RADIUS
        events.died = true
    end

    -- Tuyaux : défilement, spawn, score, collisions
    g.dist_to_next = g.dist_to_next - v * dt
    if g.dist_to_next <= 0 then
        spawn_pipe(g, w, h)
        g.dist_to_next = g.dist_to_next + PIPE_SPACING
    end

    for i = #g.pipes, 1, -1 do
        local p = g.pipes[i]
        p.x = p.x - v * dt

        if not p.scored and p.x + PIPE_WIDTH < g.bird.x then
            p.scored = true
            g.score = g.score + 1
            events.scored = true
        end

        if p.x + PIPE_WIDTH < -10 then
            table.remove(g.pipes, i)
        elseif circle_hits_rect(g.bird.x, g.bird.y, BIRD_RADIUS, p.x, 0, PIPE_WIDTH, p.gap_y)
            or circle_hits_rect(g.bird.x, g.bird.y, BIRD_RADIUS,
                p.x, p.gap_y + p.gap_h, PIPE_WIDTH, (h - GROUND_H) - (p.gap_y + p.gap_h))
        then
            events.died = true
        end
    end

    return events
end

--- Angle du sprite selon la vitesse verticale (pique du nez en tombant).
function game.bird_angle(g)
    return math.max(-0.5, math.min(1.25, g.bird.vy / 420))
end

return game
