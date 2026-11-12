# Parity architecture: module layout, ownership, contracts

Foundation for reaching parity with PokeTokenBar v2.5.7 (macOS). Six area agents build in parallel
git worktrees. Each one edits **only the files its area owns** (table in §2). Shared files are frozen
except at the marked extension points listed in §3. If you need a change to a shared file, write it up
in your PR description under "Foundation change request". Do not make the change yourself.

Area specs: `providers.md`, `limits.md`, `companion.md`, `home-usage.md`, `shop-bag-dex.md`,
`pet-settings-misc.md`. The shell and visual language are in `ui-shell.md`; this foundation implements them.

Build and test (every PR must pass all four):

```
npm run build                                                              # tsc + vite
npm test                                                                   # src/**/*.test.ts via scripts/test.mjs
(cd src-tauri && PATH=/opt/homebrew/opt/rustup/bin:$PATH cargo test)
(cd src-tauri && PATH=/opt/homebrew/opt/rustup/bin:$PATH cargo build)      # no new warnings
```

Frontend tests are plain `node:assert` files named `*.test.ts`. They run through Vite's module runner,
and `tsconfig.json` excludes them from `tsc`.

---

## 1. Module layout

```
src-tauri/src/
  lib.rs              run(): plugins, area setup order, invoke router, `area_commands!` macro
  model.rs            shared data contract: usage model, event names, PopoverOpened, DailyLedger
  store.rs            load_json / save_json (atomic, keeps .corrupt)
  format.rs           TokenFormatter port (compact, percent, cost, cost_compact, cost_text)
  settings/mod.rs     Settings = flattened per-area blocks; ShellSettings; load/store
  shell/              tray, popover window, refresh scheduler, crash.log, settings side effects
    mod.rs  tray.rs  popover.rs  i18n.rs
  providers/mod.rs    local logs → UsageSnapshot                     (providers)
  limits/mod.rs       official limits, alerts, tooltip limit line    (limits)
  companion/          egg/hatch/evolve/graduate, PokéAPI, api.rs     (companion)
    mod.rs  pokeapi.rs  api.rs
  home/mod.rs         daily ledger, streak, recap data               (home-usage)
  economy/mod.rs      wallet, shop, bag, candy grants                (shop-bag-dex)
  pet/mod.rs          floating pet, tray sprite, autostart, services (pet-settings-misc)

src/
  main.tsx            window router (#pet → Pet, else App); global CSS imports
  App.tsx             popover shell: banner, segmented tabs, tab body, footer, Settings swap
  shared.ts           backend contract types, useBackend, usePopoverOpened, updater (+ area #regions)
  i18n.ts             LEGACY ko/en/ja dictionary (frozen; read-only, being drained)
  App.css             LEGACY v0.2 component styles (frozen; areas restyle in their own CSS)
  kit/                design system: tokens.css, kit.css, components.tsx, format.ts, pace.ts,
                      colors.ts, i18n.ts, format.test.ts
  shell/              chrome.tsx (UpdateBanner, Footer), settings.tsx (screen + row registry),
                      settingsKit.tsx (row types/primitives), strings.ts, shell.css
  areas/
    home-usage/       HomeTab.tsx
    companion/        CompanionCard.tsx, settings.tsx
    limits/           LimitsSection.tsx, settings.tsx
    providers/        (empty: add settings.tsx, strings.ts, …)
    shop-bag-dex/     ShopTab.tsx, BagTab.tsx, CollectionTab.tsx
    pet-settings-misc/Pet.tsx, settings.tsx
scripts/test.mjs      frontend test runner
```

---

## 2. File ownership

| Area | Owns (may create anything under these paths) |
|---|---|
| **providers** | `src-tauri/capabilities/providers.json`, `src-tauri/src/providers/**` (pricing table goes here too: `providers/pricing.rs`), `src/areas/providers/**`, `shared.ts` `#region providers` |
| **limits** | `src-tauri/capabilities/limits.json`, `src-tauri/src/limits/**`, `src/areas/limits/**`, `shared.ts` `#region limits` |
| **companion** | `src-tauri/capabilities/companion.json`, `src-tauri/src/companion/**` (incl. `api.rs` bodies), `src/areas/companion/**`, `shared.ts` `#region companion` |
| **home-usage** | `src-tauri/capabilities/home-usage.json`, `src-tauri/src/home/**`, `src/areas/home-usage/**`, `shared.ts` `#region home-usage` |
| **shop-bag-dex** | `src-tauri/capabilities/shop-bag-dex.json`, `src-tauri/src/economy/**`, `src/areas/shop-bag-dex/**`, `shared.ts` `#region shop-bag-dex` |
| **pet-settings-misc** | `src-tauri/capabilities/pet-settings-misc.json`, `src-tauri/src/pet/**`, `src/areas/pet-settings-misc/**`, `shared.ts` `#region pet-settings-misc` |
| **foundation / shared** (frozen) | `src-tauri/src/{lib,model,store,format}.rs`, `src-tauri/src/settings/**`, `src-tauri/src/shell/**`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`, `src-tauri/Cargo.toml`*, `package.json`*, `src/{main,App}.tsx`, `src/shared.ts` (outside regions), `src/kit/**`, `src/shell/**`, `src/i18n.ts`, `src/App.css`, `scripts/**`, `docs/parity/ARCHITECTURE.md` |

\* Dependencies: an area may add a crate or npm package it alone uses. Put it on its own line at the end of
the dependency list so merges stay trivial, and say so in the PR.

Cross-area imports are allowed **read-only**: call another area's `pub` functions and render its
exported components. Never edit the other area's files.

---

## 3. Extension points (how an area plugs in without editing shared files)

### Rust

1. **Commands.** In the area's `mod.rs`, call `crate::area_commands![cmd_a, cmd_b];` once. The commands
   must be defined in (or glob re-exported into) that module. `lib.rs` routes any invoke whose name is
   in the area's `COMMANDS`. Command names must be unique app-wide.
2. **Setup.** `pub fn setup(app: &tauri::App) -> tauri::Result<()>` (companion's also takes the data dir).
   It manages the area's state and starts its threads. `lib.rs` calls the areas in this order: settings,
   providers, limits, companion, home, economy, pet, then shell last.
3. **Refresh hooks** (called by `shell::scheduler`, signatures fixed):
   - `providers::refresh(app) -> UsageSnapshot` and `providers::current(app)`
   - `limits::refresh(app) -> Vec<ProviderLimits>` and `limits::current(app)`. The limits refresh is
     throttled to at most once every 5 minutes; a manual refresh bypasses that.
   - `companion::on_usage(app, &UsageSnapshot)`. It credits growth, hatches, notifies and emits.
   - `home::on_usage(app, &UsageSnapshot)`. This is where the day ledger is recorded.
   - `economy::on_limits(app, &[ProviderLimits])`. Free Rare Candy grants go here.
   - The shell emits `usage` / `limits` and refreshes the tray after the hooks run.
4. **Tray tooltip pieces** (signatures fixed; the bodies belong to their areas):
   - `limits::menu_limit_line(app, &Settings) -> Option<String>`
   - `companion::menu_growth_text(app, lang) -> String`
5. **Settings side effects.** Every area has `pub fn on_settings(app, &Settings)` (a no-op until the
   area needs it). The shell calls all of them, in setup order, on every settings change and once at
   launch, then refreshes the tray. Pet visibility is handled in `pet::on_settings`; limits clears its
   values there when `status_checks_enabled` goes off, and so on. Keep the signature.
6. **Tray sprite.** `pet::start_tray_sprite(app)` is called once after the tray exists. Draw frames with
   `shell::tray::set_icon(app, Some(Image))`. Passing `None` restores the app icon. Pause the animation
   while `shell::popover_visible(app)` is true.
7. **Settings keys.** Each area owns a `*Settings` struct in its module. It is `#[serde(flatten)]`ed into
   `settings::Settings`, so the JSON stays flat. Add keys and the area's `sanitize()` rules only there.
   Key names must be unique across all blocks. Mirror each change in the area's `shared.ts` region type.
8. **Notifications.** Use `shell::notify(app, title, body)`.
9. **Persistence.** Use `store::{load_json, save_json}` on a file in the app data dir. Each area owns its
   own file, except economy, which writes inside `companion.json` (see §7).
10. **Capabilities.** An area that needs a Tauri permission (for example a window or plugin permission
   for its own window) adds `src-tauri/capabilities/<area>.json` with its own `identifier`. Tauri loads
   every file in that folder, so `default.json` stays frozen.

### Frontend

1. **Tab bodies.** `App.tsx` imports exactly four files, each with a default export
   `(props: TabProps) => ReactNode`:
   - `areas/home-usage/HomeTab.tsx`
   - `areas/shop-bag-dex/ShopTab.tsx`
   - `areas/shop-bag-dex/BagTab.tsx`
   - `areas/shop-bag-dex/CollectionTab.tsx`

   `TabProps` is `{usage, companion, limits, settings, save, goTo}`. The shell gives each body the space
   between the picker and the footer (about 508 px at 604 high), with a 12 px scroller inset. Bodies scroll
   internally.
2. **Home slots.** `HomeTab` (home-usage) renders `areas/companion/CompanionCard` and
   `areas/limits/LimitsSection` (default exports) as `<Slot {...props} />`, passing its whole `TabProps`.
   The slot owner reads whatever it needs from `TabProps` and fetches anything else with its own hooks,
   so it never edits `HomeTab.tsx`. Home-usage keeps both call sites with the spread props, in this
   order: CompanionCard first, LimitsSection inside the limits block (home-usage.md §5.2).
3. **Settings rows.** Each area exports `settingsRows: SettingsRow[]` from `src/areas/<area>/settings.tsx`.
   `shell/settings.tsx` collects them with `import.meta.glob`. A row is
   `{id, group, order, Row}`:
   - `group` is one of `general, difficulty, menuBar, providers, floatingPet, notifications, updates,
     backup, advanced, about`. Groups render in that order.
   - `order` is the pet-settings-misc.md §3.3 row number × 10. That keeps rows from different areas
     interleaved correctly in shared groups like General.
   - `Row` receives `SettingsRowProps` (`settings, set, save, companion, updater`).
   - Use `SettingRow` and `Toggle` from `shell/settingsKit.tsx`.
4. **Strings.** Each area creates `src/areas/<area>/strings.ts` with
   `export const xStrings = defineStrings({ en: {...}, ko: {...}, ja: {...}, es, fr, pt, de, ru })` and
   reads it with `useStrings(xStrings)` from `kit/i18n`. English is the full set; a missing key falls back
   to English. Port the 8-language text from the macOS `Localization.swift`.

   The legacy `src/i18n.ts` (`useI18n().t`) is **frozen**: read from it, never add to it. Move your
   keys into your own dictionary as you rewrite your components. The integrator deletes `i18n.ts` once
   nothing imports it.
5. **Per-open reset.** Call `usePopoverOpened(cb)` from `shared.ts` to reset in-memory UI state on each
   open (recap closed, Collection back to the Pokédex segment, …). The payload is `PopoverOpened`.
6. **Area data.** Add your own hooks (invoke plus listen on your own events) inside your area folder.
   Don't extend `useBackend`.
7. **Styles.** Put area CSS in your area folder and import it from your component. Use the variables in
   `kit/tokens.css`, never raw hex, except data colours such as type colours. `App.css` is legacy; stop
   using its classes as you restyle.

---

## 4. Tauri commands and events

### Commands (invoke name → args → result)

| Area | Command | Args | Returns |
|---|---|---|---|
| shell | `get_settings` | — | `Settings` |
| shell | `set_settings` | `{value: Settings}` | `Settings` (sanitized; also emits `settings`) |
| shell | `refresh_now` | — | `()` (limits + usage on a worker thread) |
| shell | `show_popover` | `{opened?: PopoverOpened}` | `()` (shows, never toggles off) |
| shell | `hide_popover` | — | `()` |
| shell | `set_popover_mode` | `{settings: bool}` | `()` (360×460 vs 360×604, re-anchored) |
| shell | `toggle_window` | `{label: "main" \| "pet"}` | `()` (`pet` flips `floating_pet_enabled`) |
| shell | `quit` | — | `()` |
| providers | `usage_snapshot` | — | `UsageSnapshot` |
| limits | `limits` | — | `ProviderLimits[]` |
| companion | `companion` | — | companion view JSON (`Companion` in shared.ts) |
| companion | `set_difficulty` | `{value: f64}` | companion view |
| pet | `get_autostart` / `set_autostart` | — / `{enabled: bool}` | `bool` |

### Events (Rust → webview)

| Event | Payload | Emitted by |
|---|---|---|
| `usage` | `UsageSnapshot` | shell after `providers::refresh` and its hooks |
| `limits` | `ProviderLimits[]` | shell after `limits::refresh` |
| `companion` | companion view | companion (`on_usage`, `update`, `set_difficulty`) |
| `companion-event` | `{kind: "hatched", id, shiny} \| {kind: "evolved", id} \| {kind: "graduated", id}` | companion |
| `settings` | `Settings` | shell `apply_settings` |
| `popover-opened` | `PopoverOpened {tab: "home"\|"shop"\|"bag"\|"collection"\|null, settings: bool}` | shell when the popover opens (tray click, tray menu Pokédex/Settings…, pet click, second instance) |

Event name constants are in `model.rs` (`EV_*`). New area events use an area prefix, for example
`economy-…` or `home-…`.

---

## 5. Settings schema (`settings.json`, one flat object)

| Key | Type | Default | Owner block |
|---|---|---|---|
| `language` | `ko\|en\|ja\|es\|fr\|pt\|de\|ru` | system locale, else `en` | shell |
| `refresh_minutes` | u32 (0 = manual; 0/1/2/5/15) | 2 | shell |
| `show_tokens_in_menu` (alias `tray_tokens`) | bool | true | shell |
| `show_cost_in_menu` | bool | false | shell |
| `show_growth_in_menu` | bool | false | shell |
| `show_limit_in_menu` (alias `tray_limit`) | bool | false | shell |
| `menu_limit_color_mode` | `gauge\|attention\|off` | gauge | shell |
| `update_notifications_enabled` | bool | true | shell |
| `skipped_update_version` | string? | null | shell |
| `disabled_providers` | string[] (sorted) | [] | providers |
| `custom_scan_roots` | {providerId: string} | {} | providers |
| `additional_claude_config_dirs` | string | "" | providers |
| `limit_display` | `used\|remaining` | used | limits |
| `limit_notifications` | bool | true | limits |
| `warn_threshold` / `crit_threshold` | f64 | 80 / 95 | limits |
| `status_checks_enabled` | bool | true | limits |
| `claude_tracked_account` | string | automatic | limits |
| `daily_trend_window` | `7d\|14d\|21d\|month` | 14d | home |
| `activity_visualization` | `monthlyBars\|recentHeatmap` | monthlyBars | home |
| `representative` | u32? | null | companion |
| `companion_notifications` | bool | true | companion |
| `shop_difficulty` | f64 0.1…2.0 | 1.0 | economy |
| `floating_pet_enabled` (alias `pet_enabled`) | bool | false | pet |
| `always_faces_back` | bool | false | pet |
| `floating_pet_size` | u32 48…384 | 96 | pet |
| `floating_pet_bubble_alerts` | bool | true | pet |
| `floating_pet_agent_activity` | bool | true | pet |
| `animation_quality` | `powerSaver\|balanced\|smooth` | powerSaver | pet |

Growth difficulty stays in `companion.json` (`growth_difficulty`), not in settings. The aliases read v0.2
files; the new names are written back.

---

## 6. Usage data model (`model.rs`, mirrored in `shared.ts`)

```
UsageSnapshot { providers: [ProviderSnapshot], fetched_at: rfc3339?, error: string? }
ProviderSnapshot {
  provider_id,              // macOS ids: claude_code, codex, gemini, antigravity, opencode, hermes,
                            // cursor, grok, copilot, kiro, pi, omp, omo, aside, kimi_code, devin
  display_name,
  today: DailyUsage?,       // date == today's local yyyy-MM-dd, else ignored by totals
  active_block: ActiveBlock?, week_total: PeriodUsage?, month_total: PeriodUsage?,
  month_daily: [DayPoint], recent_daily: [DayPoint],   // explicit zero days
  last_usage: rfc3339?, reports_cost: bool }
Usage       { tokens: Tokens{input, output, cache_write, cache_read}, total, cost: f64, coverage: CostCoverage }
CostCoverage{ reported, estimated, unknown }   // provenance flags, merged with OR
DailyUsage  = Usage + { date, models: [ModelUsage]? }   // per-model tokens + cost + coverage; None = not opted in
PeriodUsage = Usage + { from, to }
ModelUsage  = Usage + { model }
DayPoint    { date, total, cost }
ActiveBlock { start, end, total, tokens_per_minute }
DailyLedger { tokens_by_day: {yyyy-MM-dd: u64}, covered_since? }   // home-usage, usage-ledger.json
```

Helpers: `today_total(day)`, `today_cost(day) -> (usd, coverage)`, `today_by_provider(day)` (used for
companion crediting), and `shows_cost()`. Frontend helpers are `todayKey()` and `todayTotal(u)`.

Today the providers area emits only `claude_code` and `codex`, with `today` set and
`reports_cost = false`. While `reports_cost` is false the tray and Home keep cost hidden. Flip it per
provider once the price table lands. The price table, providers.md §3.4 and home-usage.md §3.3, belongs
to providers in `providers/pricing.rs`. Home-usage only formats cost.

Cost text everywhere uses `format::cost_text` (Rust) or `costText` (kit). Unknown-only coverage shows
`$—` in compact form or the localized "Unavailable".

Limits ids today are `claude` and `codex`, not the provider ids. The limits area should move them to
`claude_code` and `codex` when it reworks `ProviderLimits`, and keep `provider`, `windows[].key`,
`windows[].used_percent` and `windows[].resets_at`, because economy reads them for candy grants.

---

## 7. Companion and economy state boundary

- There is one file and one lock: `companion.json`, behind `companion::SharedStore` (a `Mutex<Store>`).
- `companion::State` (owned by companion) holds growth, egg, active Pokémon, dex, `collected_finals`,
  difficulty, and the hatch inputs: `egg_tier`, `pending_hatch_id`, forms, profiles (companion adds these).
- `economy::EconomyState` (owned by shop-bag-dex) is `#[serde(flatten)]`ed into `State` as
  `state.economy`. It holds `spent_tokens`, `inventory`, candy grant tiers and epochs, and dex completion
  notification keys. The economy agent adds fields there. Names must not collide with companion's.
- Economy mutates state only inside `companion::update(app, |state| …)`. That call saves and emits
  `companion`.
- Economy calls companion behaviour only through `companion/api.rs`:
  - `lifetime_tokens()`: the wallet income side. Spendable = `lifetime_tokens - economy.spent_tokens`.
  - `apply_candy_xp(xp, rng)`: Rare Candy.
  - `reroll_nature(rng)`: Mint. Stub until companion fills it in.
  - `release_active_for_egg(guarantee)`: egg purchase. Stub until companion fills it in.

  Companion owns the bodies. Economy may rely on the signatures. A new call needs a signature added to
  `api.rs` by the companion agent; economy asks for it in its PR.
- Companion reads economy state where the rules need it (Shiny Charm odds read
  `self.economy.inventory["shinyCharm"]`). Economy exposes a helper for that on `EconomyState`.
- The economy view for the UI comes from economy's own command and event (`economy-…`). It does not go
  through the companion view.

---

## 8. Shell behaviour already implemented (for reference)

- **Popover.**
  - Window `main`: undecorated, shadowed, 360×604 (Settings 360×460 via `set_popover_mode`). Hidden at
    launch, always on top, no taskbar button.
  - Anchored to the tray icon rect (`tray.rect()`), clamped to the monitor work area with an 8 px
    margin; placed above the icon when it is in the lower half of the monitor.
  - Hides on blur after a 120 ms debounce. A tray click within 250 ms of a blur-hide is ignored.
  - Excluded from window-state persistence. The pet window keeps its position.
  - Each open emits `popover-opened`, which resets the UI to Home with Settings closed.
- **Footer.** On Home only: refresh, "Updated N", and an error triangle when `usage.error` is set. On
  all tabs: the pet eye toggle, the gear, and power.
- **Tray.**
  - Left click toggles the popover.
  - The right-click menu is rebuilt on every refresh or settings change, in all 8 languages: summary
    row (disabled), Refresh now, Pokédex (opens Collection), Settings…, Hide/Show floating pet, Show
    back view (check item), Quit.
  - The tooltip is `"PokeTokenBar\n" + menu_lines`, capped at 127 characters.
- **Single instance.** A second launch opens the running copy's popover.
- **Crash log.** Panics append to `crash.log` in the app data dir.
- **Minimized-window fix.** `shell::on_screen` and `shell::reveal`.
- **Kit.**
  - `tokens.css`: light and dark tokens.
  - `components.tsx`: `Badge`, `TypeBadges`, `RarityBadge`, `CapsuleTabBar`, `LimitProgressBar` with a
    pace marker, `Tooltip` (600 ms), `Icon`, `IconButton`, `Segmented`.
  - `format.ts`: `compact`, `grouped`, `cost`, `costCompact`, `percent`, `costText`, `relativeDuration`.
  - `pace.ts`: `tier`, `gauge`, `isCalm`, `paceFraction`.
  - `colors.ts`: tier, rarity and type colours.
  - `i18n.ts`: 8 languages with English fallback.

## 9. Known gaps left to areas (not foundation work)

| Area | Gaps |
|---|---|
| providers | Every provider beyond `claude_code`/`codex`; week, month and daily series; the cost and price table; `error`; the scan cache. |
| limits | Per-provider headline and pace in `menu_limit_line`; the "used today" filter; ids; the `LimitsSection` restyle using `LimitProgressBar` and `pace.ts`; es–ru notification text. |
| companion | `api.rs` stubs; forms, profiles and Ditto; the `CompanionCard` restyle; es–ru notification text. |
| home-usage | The ledger in `home::on_usage`; trend, heatmap and recap; the per-model list. |
| shop-bag-dex | `ShopTab` and `BagTab`; economy commands; candy grants in `economy::on_limits`; regions, badges and the detail page in `CollectionTab`. |
| pet-settings-misc | The tray sprite (`start_tray_sprite`); menu bar cost and growth rows; `floating_pet_size` wiring; pet features; save transfer and snapshots; the "skip version" row; the deep link. |

Open question kept from ui-shell.md §8 Q4: the Windows-only hatch/evolve toast stays in `App.tsx` for
now. The companion area can propose removing it in its PR.
