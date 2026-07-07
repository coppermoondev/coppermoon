# AGENTS.md - Guide complet pour developper dans CopperMoon

Ce document est la reference operationnelle pour les prochains agents.
But: produire des changements fiables, testables, et coherents avec l'architecture CopperMoon.

## 1) Mission et principes

Quand tu travailles sur CopperMoon:

- preserve la compatibilite des CLI (`coppermoon`, `harbor`, `shipyard`, `quarry`)
- fais des changements cibles (pas de refactor global non demande)
- ajoute des tests a chaque comportement modifie
- valide avec un smoke test reel (commande CLI) quand c'est pertinent
- documente ce qui n'a pas pu etre verifie

## 2) Carte de l'architecture

### Workspace Rust (racine)

- `crates/coppermoon`: binaire runtime Lua
- `crates/coppermoon_core`: runtime, loader `require`, event loop/timers
- `crates/coppermoon_std`: stdlib exposee a Lua (`fs`, `http`, `net`, etc.)
- `crates/harbor`: package manager
- `crates/shipyard`: toolchain dev/build
- `crates/quarry`: process manager/daemon
- `crates/sqlite`, `crates/mysql`, `crates/postgresql`: modules DB globaux

### Ecosysteme Lua

- `packages/*`: packages Lua (certains avec extension native Rust)
- `apps/*`: applications d'exemple (patterns de reference)
- `harbor-registry/*`: app registry complete (API + storage + auth)

### Loader `require` (important)

Le loader runtime cherche les modules Lua dans:

- `<base>/?.lua`
- `<base>/?/init.lua`
- `<base>/harbor_modules/?.lua`
- `<base>/harbor_modules/?/init.lua`

Les libs natives (`.dll/.so/.dylib`) sont cherchees dans:

- `harbor_modules/<module>/native/<lib>`
- `<module>/native/<lib>`
- `native/<lib>`
- scan de `harbor_modules/*/native/<lib>` (wrapper Lua + nom de lib different)

Sur Windows, `lua54.dll` est precharge pour fiabiliser le chargement natif.

### Stdlib CopperMoon (globale, sans `require`)

Important: les modules de la stdlib sont injectes dans l'environnement global par le runtime.
Tu n'as pas besoin de faire `require("fs")`, `require("json")`, etc.

Modules globaux disponibles directement:

- `fs`
- `path`
- `os_ext`
- `process`
- `json`
- `crypto`
- `time`
- `http` (avec `http.server`)
- `net` (avec `net.ws`)
- `buffer`
- `term`
- `console`
- `archive`
- `re`

Fonctions/valeurs globales utiles:

- `print(...)` (version enrichie CopperMoon)
- `_COPPERMOON_VERSION`
- `setTimeout(fn, ms)`
- `setInterval(fn, ms)`
- `clearTimeout(id)`
- `clearInterval(id)`

Extensions sur libs Lua built-in (pas de `require` non plus):

- `string.*` enrichi (`split`, `trim`, `starts_with`, `ends_with`, `contains`, etc.)
- `table.*` enrichi (`keys`, `values`, `map`, `filter`, `reduce`, `contains`, etc.)

Exemple correct:

```lua
print("CopperMoon:", _COPPERMOON_VERSION)

fs.write("hello.txt", "hello")
local txt = fs.read("hello.txt")
print(txt)

local obj = json.decode('{"ok":true}')
print(obj.ok)

local timer = setTimeout(function()
    print("timer done")
end, 100)
clearTimeout(timer)
```

Ce qui doit utiliser `require(...)`:

- packages ecosysteme (`require("i18n")`, `require("honeymoon")`, etc.)
- modules/app locaux (`require("lib.service")`, etc.)

Note DB: `sqlite`, `mysql`, `postgresql` sont aussi enregistres globalement par le runtime principal
(ce ne sont pas des modules de la stdlib, mais ils sont disponibles sans `require` dans `coppermoon` CLI).

### Event loop asynchrone (important)

Le runtime execute le chunk principal, les callbacks de timers et les handlers HTTP
sur une event loop mono-thread (une `LocalSet` Tokio pilotee par `coppermoon_core::run_local`,
sans jamais entrer dans le garde bloquant de Tokio). Consequences:

- `time.sleep`, `http.*` (client) et les handlers `http.server` sont asynchrones de facon
  transparente: pendant qu'un handler attend une I/O, les autres requetes et les timers continuent
- les handlers HTTP s'executent en concurrence (entrelacement type Node.js), mais toujours
  sur un seul thread: pas de data race possible cote Lua
- `require` est implemente en pur Lua (module.rs): le top-level des modules est **yieldable**
  — les modules peuvent faire du http, `time.sleep`, des requetes DB au chargement
  (pattern `models/init.lua` + `db:autoMigrate` de copper-blog)
- les callbacks de timers s'executent en **concurrence** (chaque callback est spawn sur
  l'event loop): un `setTimeout` suspendu sur une I/O ne retarde pas les autres timers
- `net` (TCP/UDP) est en vraie I/O async tokio; `archive` (zip/tar/gzip) part sur le pool
  blocking (spawn_blocking); les bindings DB `sqlite`/`mysql`/`postgresql` executent leurs
  requetes via spawn_blocking — l'event loop continue de servir pendant une query
- dans un contexte **non-yieldable** (metamethodes, comparateurs `table.sort`...), `time.sleep`,
  `http.*` et les methodes archive basculent automatiquement sur un chemin bloquant
  (shim hybride base sur `coroutine.isyieldable()`) — resultat identique, event loop en pause
- REGLE pour les methodes async d'userdata (`add_async_method`): les borrows d'userdata mlua
  sont exclusifs — cloner les champs Arc necessaires puis `drop(this)` AVANT le premier
  `.await`, sinon "error borrowing userdata" des que deux coroutines partagent l'objet
  (voir crates/sqlite/src/lib.rs comme reference du pattern complet)
- regle d'or: le code Lua ne tourne QUE sur le thread principal; `coppermoon_core::block_on`
  reste utilisable dans les fonctions sync (le thread principal n'est jamais "entre" dans Tokio,
  donc pas de panic de runtime imbrique — y compris pour `postgres` qui cree son propre runtime)

### Serveur HTTP et arret gracieux (prod)

- `server:listen(port, host?, callback?)` — host optionnel (string), sinon env `COPPERMOON_HOST`,
  sinon `127.0.0.1`. En Docker, l'image definit `COPPERMOON_HOST=0.0.0.0`
- keep-alive HTTP/1.1 par defaut (`Connection` respecte, max 1000 requetes/connexion,
  timeout idle 30s), bodies `Transfer-Encoding: chunked` decodes (limite 10 Mo)
- arret gracieux: Ctrl+C / SIGTERM (ou `process.shutdown()` depuis Lua) → le serveur cesse
  d'accepter, draine les requetes en vol (deadline 10s), `listen` retourne, le process sort
  proprement (exit 0). Un second signal force la sortie (exit 130)
- erreurs de handler/timer: routees vers `process.on_error(fn)` si installe (recoit
  `(message, context)` avec context = "http handler" | "timer"), sinon loggees avec stack
  traceback Lua; la requete recoit un 500, le process continue. `ctx:json`/`json.encode`/`print`
  sont proteges contre les tables cycliques (garde de profondeur, pas de stack overflow)
- les chunks sont nommes avec le prefixe `@` → les tracebacks affichent `fichier.lua:ligne`

### Limites et observabilite (prod)

Variables d'environnement:

- `COPPERMOON_MEMORY_LIMIT` (ex. `512M`, `1G`) — limite memoire du VM Lua; depassement =
  erreur Lua catchable (`not enough memory`), pas d'OOM process
- `COPPERMOON_MAX_IN_FLIGHT` (defaut 1024) — handlers HTTP concurrents; au-dela → 503
  immediat avec `Retry-After: 1` (load shedding, pas de file illimitee)
- `COPPERMOON_MAX_CONNECTIONS` (defaut 10240) — connexions TCP simultanees; au-dela → 503
- `COPPERMOON_HOST` — adresse d'ecoute par defaut du serveur

APIs Lua:

- `http.server.stats()` → `{ requests_total, rejected_total, in_flight, uptime_seconds,
  status_1xx..status_5xx }` — pour construire un endpoint `/healthz` applicatif
- `process.memory()` → `{ used, limit }` (octets, VM Lua)
- `process.on_error(fn | nil)` — hook d'erreurs non-catchees
- `process.shutdown()` — arret gracieux programmatique

TLS: non gere par le serveur embarque — deploiement supporte = reverse proxy (nginx, Caddy,
Traefik) devant CopperMoon, qui termine TLS et parle HTTP/1.1 keep-alive au runtime.

## 3) Commandes de base (toolchain complete)

### Build/test workspace

```bash
cargo build
cargo test --workspace
cargo build --release
```

### CopperMoon

```bash
coppermoon app.lua
coppermoon run app.lua -- arg1 arg2
coppermoon repl
coppermoon version
```

### Harbor

```bash
harbor init
harbor install
harbor install honeymoon
harbor install user/repo@branch:main
harbor install user/repo@tag:v0.1.0
harbor install ../local-package
harbor uninstall honeymoon
harbor update
harbor list
harbor rebuild
harbor rebuild redis
```

### Shipyard

```bash
shipyard new my-app --template web
shipyard init --template api
shipyard dev --port 3000
shipyard run --file app.lua
shipyard build
shipyard scripts
shipyard script test
```

### Quarry

```bash
quarry start apps/coppermoon-dev/app.lua --name coppermoon-dev
quarry list
quarry logs coppermoon-dev -f
quarry info coppermoon-dev
quarry restart coppermoon-dev
quarry stop coppermoon-dev
quarry delete coppermoon-dev
quarry save
quarry resurrect
quarry startfile --config quarry.toml
```

## 4) Playbooks selon le type de changement

### A. Changement runtime/std (Rust)

1. Modifier uniquement le crate cible (`coppermoon_core` ou `coppermoon_std`).
2. Ajouter/adapter tests unitaires Rust si logique interne.
3. Faire un smoke test Lua depuis `coppermoon`.
4. Verifier regressions sur `require` et erreurs runtime.

Smoke minimum:

```bash
cargo build
cargo test -p coppermoon_core
cargo test -p coppermoon_std
coppermoon apps/coppermoon-dev/app.lua
```

### B. Changement Harbor/Shipyard/Quarry

1. Verifier signatures CLI existantes.
2. Ajouter test unitaire si dispo et au moins un test de commande reel.
3. Verifier messages d'erreur et cas non happy-path.

Smoke minimum:

```bash
cargo build -p harbor
cargo build -p shipyard
cargo build -p quarry
```

### C. Changement package Lua (`packages/*`)

1. Mettre a jour `harbor.toml` du package si necessaire.
2. Ajouter/mettre a jour tests Assay (`tests/*`).
3. Valider execution via `coppermoon tests/init.lua`.
4. Verifier import package depuis un projet consommateur.

## 5) Developper un package Harbor (Lua pur)

Structure recommandee:

```text
my-package/
  harbor.toml
  init.lua
  lib/
    ...
  tests/
    init.lua
    *_test.lua
```

Exemple `harbor.toml`:

```toml
[package]
name = "my-package"
version = "0.1.0"
description = "My CopperMoon package"
author = "Team"
license = "MIT"
main = "init.lua"

[dependencies]

[dev-dependencies]
assay = { path = "../assay" }

[scripts]
test = "coppermoon tests/init.lua"
```

## 6) Developper un package natif (Lua + Rust)

Exemple minimal:

`harbor.toml`

```toml
[package]
name = "hello_native"
version = "0.1.0"
main = "init.lua"

[native]
build = true
```

`Cargo.toml`

```toml
[package]
name = "hello_native"
version = "0.1.0"
edition = "2021"

[workspace]

[lib]
name = "hello_native"
crate-type = ["cdylib"]

[dependencies]
mlua = { version = "0.10", features = ["lua54", "module"] }
```

`src/lib.rs`

```rust
use mlua::prelude::*;

#[mlua::lua_module]
fn hello_native(lua: &Lua) -> LuaResult<LuaTable> {
    let m = lua.create_table()?;
    m.set("add", lua.create_function(|_, (a, b): (f64, f64)| Ok(a + b))?)?;
    m.set("greet", lua.create_function(|_, name: String| Ok(format!("Hello, {}!", name)))?)?;
    Ok(m)
}
```

`init.lua` (wrapper Lua)

```lua
local native = require("hello_native")

return {
    add = native.add,
    greet = native.greet,
}
```

Install et test:

```bash
harbor install ../hello-native
coppermoon smoke.lua
```

`smoke.lua`:

```lua
local m = require("hello_native")
print(m.add(2, 3))
print(m.greet("Ada"))
```

Note: Harbor construit le natif avec `cargo build --release` et copie la lib dans `native/`.

## 7) Guidelines tests Assay (obligatoire)

Cette section suit les patterns reels de `packages/i18n/tests` et `packages/std-tests/tests`.

### 7.1 Convention recommandee

- un fichier `tests/init.lua` qui orchestre le run multi-fichiers
- des fichiers `*_test.lua` independants
- chaque fichier retourne `Assay.runner:run()` ou `Assay.run()`
- reset du runner entre fichiers dans `tests/init.lua`

### 7.2 Bootstrap `tests/init.lua` (pattern i18n/std-tests)

Utilise ce pattern pour fiabiliser `require(...)` en contexte package:

```lua
local sep = package.config:sub(1, 1)
package.path = ".." .. sep .. "?" .. sep .. "init.lua;" ..
               ".." .. sep .. "?.lua;" ..
               package.path

local function luaSearcher(modname)
    local path = modname:gsub("%.", "/")
    for pattern in package.path:gmatch("[^;]+") do
        local filepath = pattern:gsub("%?", path)
        local fn = loadfile(filepath)
        if fn then
            return fn, filepath
        end
    end
end
table.insert(package.searchers, 2, luaSearcher)

local Assay = require("assay")
Assay.configure({ bail = false, verbose = true, colors = true })

local allResults = { total = 0, passed = 0, failed = 0, skipped = 0 }
local testFiles = {
    "my-package.tests.unit_test",
    "my-package.tests.integration_test",
}

for _, testFile in ipairs(testFiles) do
    Assay.runner.reset()
    local ok, results = pcall(function() return require(testFile) end)
    if ok and type(results) == "table" then
        allResults.total = allResults.total + (results.total or 0)
        allResults.passed = allResults.passed + (results.passCount or 0)
        allResults.failed = allResults.failed + (results.failCount or 0)
        allResults.skipped = allResults.skipped + (results.skipCount or 0)
    else
        print("Error loading " .. testFile .. ": " .. tostring(results))
        allResults.failed = allResults.failed + 1
    end
end

return allResults.failed == 0 and 0 or 1
```

### 7.3 Exemple de test unitaire Assay

```lua
local Assay = require("assay")
local mymod = require("my-package.lib.mymod")

Assay.global()

describe("mymod.normalize", function()
    it("normalizes input", function()
        expect(mymod.normalize("  Hello  ")):toBe("hello")
    end)

    it("throws on invalid type", function()
        expect(function()
            mymod.normalize(42)
        end):toThrow()
    end)
end)

return Assay.runner:run()
```

### 7.4 Exemple integration test (style i18n)

Le package `i18n` montre une bonne pratique: tester un comportement integrateur complet
avec doubles/fakes d'objets applicatifs.

```lua
local Assay = require("assay")
local i18n = require("i18n")
Assay.global()

describe("middleware integration", function()
    it("binds req/res helpers", function()
        local translator = i18n.new({
            locale = "en",
            resources = {
                en = { common = { greeting = "Hello {{name}}" } },
                fr = { common = { greeting = "Bonjour {{name}}" } },
            },
        })

        local req = {
            query = { lang = "fr" },
            headers = { ["accept-language"] = "en-US,en;q=0.9" },
            get = function(self, name) return self.headers[string.lower(name)] end,
        }
        local res = {
            locals = {},
            _headers = {},
            set = function(self, name, value) self._headers[name] = value end,
        }

        local nextCalled = false
        local middleware = translator:honeymoon({
            queryParam = "lang",
            supportedLocales = { "en", "fr" },
            setContentLanguage = true,
        })

        middleware(req, res, function() nextCalled = true end)

        expect(nextCalled):toBe(true)
        expect(req.locale):toBe("fr")
        expect(req.t("greeting", { name = "Sam" })):toBe("Bonjour Sam")
        expect(res._headers["Content-Language"]):toBe("fr")
    end)
end)

return Assay.runner:run()
```

### 7.5 Bonnes pratiques de tests Assay

- utilise `Assay.global()` dans chaque fichier de test
- fais des `describe` par fonctionnalite, `it` par comportement observable
- couvre success + erreurs + cas limites
- isole les effets de bord (`beforeAll/afterAll` pour setup/cleanup)
- pour `fs`, utilise un dossier temporaire dedie et supprime-le en teardown
- evite les tests dependants de l'ordre
- reset runner entre fichiers (`Assay.runner.reset()`)
- return de script test: `0` si succes, `1` sinon

### 7.6 Execution des tests

Depuis un package:

```bash
coppermoon tests/init.lua
```

Via script Shipyard (si `Shipyard.toml` present):

```bash
shipyard script test
```

Exemple reel du repo:

- `packages/i18n/tests/init.lua`
- `packages/std-tests/tests/init.lua`

## 8) Standards de code

### Lua

- API publique stable dans `init.lua`, logique dans `lib/*`
- erreurs explicites et messages actionnables
- evite globales implicites
- prefere fonctions pures pour logique core
- garde les wrappers framework (`honeymoon`, `vein`) dans `lib/integrations/*`

### Rust

- respecte le pattern existant des crates CopperMoon
- types d'erreur clairs (`anyhow`/`thiserror` selon crate)
- tests unitaires pour parsing/validation/core logic
- modules bien limites (pas de couplage transversal inutile)

## 9) Tooling configs de reference

### `harbor.toml` avec toutes les sources

```toml
[package]
name = "my-app"
version = "0.1.0"
main = "app.lua"

[dependencies.honeymoon]
version = "0.1.0"

[dependencies.vein]
git = "https://github.com/coppermoondev/vein.git"
branch = "main"

[dependencies.my_local_pkg]
path = "../my-local-pkg"

[dev-dependencies.assay]
version = "0.1.1"
```

### `Shipyard.toml` minimal

```toml
name = "my-app"
version = "0.1.0"

[server]
port = 3000
workers = 4
host = "127.0.0.1"

[lua]
version = "5.4"
entry = "app.lua"

[scripts]
test = "coppermoon tests/init.lua"
dev = "shipyard dev"
```

### `quarry.toml` ecosystem

```toml
[[apps]]
name = "api"
script = "app.lua"
cwd = "E:/Development/my-app"
watch = false
max_restarts = 16
restart_delay = 1000
min_uptime = 5000
kill_timeout = 5000
args = ["--env", "prod"]

[apps.env]
PORT = "3000"
APP_ENV = "production"
```

## 10) Stack web: Freight + HoneyMoon + Vein

Cette stack est le pattern web principal dans CopperMoon:

- `honeymoon`: routing, middleware, validation, sessions, responses
- `freight`: acces DB/ORM, models, relations, migrations
- `vein`: templates serveur (`res:render`) et filtres/helpers

### Architecture recommandee

Separation claire des couches:

- `models/*`: definition des modeles Freight + relations + migrations
- `app.lua` / `routes/*`: routes HoneyMoon, validation, orchestration
- `views/*`: templates Vein uniquement (pas de logique DB)

Pattern reel de reference: `apps/copper-blog/app.lua` + `apps/copper-blog/models/init.lua`.

### Bootstrap type

```lua
local honeymoon = require("honeymoon")
local freight = require("freight")

local app = honeymoon.new()
app:set("env", "development")

-- Vein
app.views:use("vein")
app.views:set("views", "./views")
app.views:set("cache", false)
app.views:global("site", { name = "My App" })

-- HoneyMoon middleware
app:use(honeymoon.logger())
app:use(honeymoon.cors())
app:use(honeymoon.json())
app:use(honeymoon.helmet())

-- Freight
local db = freight.open("sqlite", { database = "./data/app.db" })
```

### Modeles Freight (pattern)

```lua
local User = db:model("users", {
    id = { type = "integer", primaryKey = true, autoIncrement = true },
    email = { type = "string", size = 255, unique = true, notNull = true },
    display_name = { type = "string", size = 100, notNull = true },
    created_at = { type = "datetime", default = "CURRENT_TIMESTAMP" },
})

User:beforeCreate(function(data)
    data.created_at = os.date("!%Y-%m-%dT%H:%M:%SZ")
end)

db:autoMigrate(User)
```

### Route HoneyMoon + validation + rendu Vein

```lua
local createUserSchema = honeymoon.schema({
    email = honeymoon.preset("email"),
    display_name = { type = "string", required = true, min = 2, max = 100, trim = true },
})

app:get("/users", function(req, res)
    local users = User:orderBy("id", "DESC"):limit(50):findAll()
    res:render("pages/users", { title = "Users", users = users })
end)

app:post("/api/users", function(req, res)
    local data = req:validate(createUserSchema)
    local user = User:create(data)
    res:status(201):json({ user = user })
end)
```

### Vein: filtres et template

```lua
app.views:filter("upper", function(s)
    return (s or ""):upper()
end)
```

```vein
{% extends "layouts/base" %}
{% block content %}
<h1>{{ title }}</h1>
{% if users and #users > 0 then %}
  <ul>
  {% for _, u in ipairs(users) do %}
    <li>{{ u.display_name | upper }} ({{ u.email }})</li>
  {% end %}
  </ul>
{% else %}
  <p>No users yet.</p>
{% end %}
{% endblock %}
```

### Guidelines importantes pour cette stack

- mets toute logique DB dans `models/*` (pas dans les templates)
- fais la validation d'entree au niveau route (`req:validate(...)`)
- garde `res:json(...)` pour API et `res:render(...)` pour pages HTML
- configure explicitement Vein (`app.views:use("vein")`) avant tout `res:render`
- ajoute pagination/limit sur les listes DB (evite `findAll()` non borne en prod)
- pour migrations auto (`db:autoMigrate(...)`), valide l'impact schema avant prod
- en cas de relation complexe, prefere methodes de modele explicites aux requetes inline dans les routes

## 11) Definition of done (DoD)

Avant de conclure un changement:

1. build/tests passes sur la zone modifiee
2. au moins un smoke test CLI pertinent execute
3. docs/config examples mis a jour si interface change
4. impacts et limites explicitement notes

## 12) Pieges frequents

- oublier `Assay.runner.reset()` dans un runner multi-fichiers
- tests qui ecrivent dans le repo sans cleanup
- casser resolution `require` en modifiant `package.path` sans searcher adapte
- supposer que `harbor install` ne build pas les modules natifs
- oublier qu'un package natif doit livrer sa lib dans `native/`

## 13) Fichiers de reference a lire en priorite

- `crates/coppermoon_core/src/module.rs`
- `crates/coppermoon_core/src/runtime.rs`
- `crates/coppermoon_std/src/lib.rs`
- `crates/harbor/src/config.rs`
- `crates/harbor/src/commands.rs`
- `crates/harbor/src/package.rs`
- `crates/shipyard/src/commands.rs`
- `crates/quarry/src/commands.rs`
- `packages/i18n/tests/init.lua`
- `packages/i18n/tests/integration_test.lua`
- `packages/std-tests/tests/init.lua`
- `packages/assay/README.md`
- `apps/copper-blog/app.lua`
- `apps/copper-blog/models/init.lua`

En cas de doute, reproduis d'abord les patterns deja utilises dans `packages/i18n` et `packages/std-tests`.
