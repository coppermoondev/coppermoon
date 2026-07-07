# 🌙 FlappyMoon

Un Flappy Bird complet en ~400 lignes de Lua, propulsé par
[Andesite](https://github.com/coppermoondev/andesite) — la couche desktop de
CopperMoon.

```bash
andesite dev examples/flappymoon      # jouer
andesite build examples/flappymoon    # → dist/flappymoon.exe autonome
andesite package examples/flappymoon  # → dist/flappymoon-setup.exe installable
```

**Contrôles** : `ESPACE` ou clic gauche. **`T`** ouvre la fenêtre de tuning
façon Dear ImGui : gravité, impulsion, vitesse, espacement… réglables **en
pleine partie**, mode invincible, graphe de fps en direct.

## Ce que l'exemple montre

| Toolkit | Usage dans le jeu |
|---|---|
| `app:on_frame(dt, ui)` + painter | boucle de jeu 60 fps, tuyaux, sol défilant |
| `andesite.image` (uv + rotation) | oiseau animé (spritesheet 2 frames) qui pique du nez |
| `andesite.audio` | sons flap / score / crash |
| `andesite.input` | clavier + souris avec détection de front |
| `andesite.storage` | meilleur score persistant entre les sessions |
| `andesite.notify` | toast système sur nouveau record |
| `andesite.clipboard` | bouton « Copier le score » |
| `app:on_ui(ui)` | menus en widgets par-dessus la scène |
| `ui:window` + `grid` + `drag_value` + `plot` | fenêtre de tuning flottante (T) : réglages live + fps |
| `ui:wants_pointer()` | le clic dans la fenêtre de tuning ne fait pas voler l'oiseau |
| `require` + `lib/` | logique de jeu séparée du rendu |

## Zéro asset binaire dans le repo

`lib/assets.lua` **génère** les sons (WAV, synthèse avec enveloppe et balayage
de fréquence) et le spritesheet (BMP 24 bits, pixel-art déclaré en ASCII) au
premier lancement, en pur Lua avec `string.pack` + `fs` — une démo de la
stdlib CopperMoon en soi.
