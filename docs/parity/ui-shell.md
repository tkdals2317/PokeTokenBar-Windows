# Parity spec: ui-shell (app shell, menu bar label, visual language)

Source of truth: PokeTokenBar v2.5.7 (macOS, Swift). Paths below are relative to `Sources/PokeTokenBar/`.
Target: PokeTokenBar-Windows (Tauri 2), `src-tauri/src` + `src`.

Files covered: `PokeTokenBarApp.swift`, `UI/PopoverView.swift` (shell parts: navigation, tab picker, Home
frame, footer, update banner, shared widgets `CapsuleTabBar`/`LimitProgressBar`/`PaceTier.color`),
`UI/StatusItemMenu.swift`, `UI/MenuBarContent.swift`, `UI/MenuLimitColoring.swift`, `UI/Badge.swift`,
`UI/PokemonNameLabel.swift`, `UI/SpriteAnimation.swift`, `OutsideClickMonitor.swift`, plus the
`UsageStore`/`TokenFormatter`/`PaceTier`/`CompanionStore` members the shell reads.
Home-tab body content (companion header, activity chart, limits rows), Shop/Bag/Collection bodies and the
Settings groups are owned by other parity docs; this doc only fixes their frame (size, position, entry/exit).

---

## 1. Overview

macOS app is a menu-bar-only accessory app (`NSApp.setActivationPolicy(.accessory)`, PokeTokenBarApp.swift:98;
`_EmptyScene()` — no windows, :14). Its whole UI is:

1. **Status item** (NSStatusItem, variable length): an animated 22pt-high Pokémon sprite + 0–2 lines of text
   (today's tokens / today's cost / growth % / limit %), limit items colored by pace tier.
2. **Left click** → a transient **NSPopover** 360pt wide, anchored under the status item, containing
   a segmented tab picker (Home / Shop / Bag / Collection), a fixed 520pt-high content area and a footer
   (refresh, "Updated N sec", error triangle, pet eye toggle, gear, power). Settings and the Usage Recap
   replace the popover content in place (no separate window, no sheet).
3. **Right click / Ctrl+click** → a native NSMenu (summary row, Refresh, Pokédex, Settings…, floating pet
   toggle, Show back view ✓, Quit).
4. A separately-owned **floating pet** NSPanel (other doc) can open the popover (`onOpenPopover`).

Windows equivalents: tray icon (animated sprite as icon) + tooltip carrying the label text; an undecorated
always-on-top 360×604 webview window positioned next to the tray that hides on blur; tray context menu with
the same 7 entries.

---

## 2. Features (numbered)

1. Single instance: a second launch yields to the already-running one and terminates **before** creating the
   status item (no icon flicker) (PokeTokenBarApp.swift:84-91). The yielding instance does nothing visible.
2. Accessory activation policy — no Dock icon, no app menu (:98).
3. Tooltip delay registered to 600 ms (`NSInitialToolTipDelay`, :101) — applies to every `.help()` tooltip.
4. Status item: sprite (animated) + text label; text recomputed on every observed change of usage,
   settings, companion growth, color mode/thresholds (`observeStore`, :142-156).
5. Status item sprite follows `companion.representativeSubject` (species, shiny, form, back-facing) and the
   animation-quality frame floor; egg glyph (🥚) while no species (:316-359, :564-574).
6. Sprite animation pauses when display sleeps, menu bar occluded, or popover is open (:488-494, :697-719).
7. Low-power mode caps animation to the power-saver floor (0.4 s) without changing the saved setting (:298-301; UsageStore.swift:150-152).
8. Left click toggles popover; right click or Control+left click shows the status menu (StatusItemMenu.swift:48-54).
9. Popover always reopens on Home with Settings/Recap closed (`PopoverNavigation.reset`, PopoverView.swift:56-61); provider tab and Claude account tab selections persist across openings (in-memory only).
10. Popover is transient; plus a global mouse-down monitor closes it on clicks into other apps' menu-bar popups (:663-677).
11. Popover content is built on open and released on close (CPU) (:586-590, :657-661).
12. Opening the popover: activates app, requests notification permission if needed, re-checks for updates (debounced) (:641-647).
13. Top segmented tabs Home/Shop/Bag/Collection; each tab body has fixed height 520pt.
14. Footer: refresh / Updated-relative-time / error triangle (Home tab only); pet eye toggle, Settings gear, Quit power (all tabs).
15. Update banner at the top of the main content when an update is available and `updateNotificationsEnabled`.
16. Settings opens inside the popover (replaces content, 360×460), with "‹ Back" (Esc) returning to the previous tab.
17. Status menu actions: Refresh, open Pokédex (Collection tab → Pokédex segment), open Settings, toggle floating pet, toggle always-back-view, Quit.
18. Menu bar tooltip: only when >1 Claude account and the menu bar shows a Claude %: "Tracked Claude account: <title>".
19. Shared visual components: `Badge` (type/rarity/status capsules), `TypeBadges`, `PokemonNameLabel` (lazy localized PokéAPI names), `CapsuleTabBar`, `LimitProgressBar` with pace marker.
20. GIF decode + frame-rate cap that preserves playback speed (`GIFDecoder.capFrameRate`).
21. URL scheme `poketokenbar://agent…` handled via Apple Event without activating the app (:61-77) — forwarded to floating pet (other doc).

---

## 3. Rules & constants

### 3.1 Menu bar text lines — `UsageStore.menuLines(growthText:)` (UsageStore.swift:354-373)

Inputs: `showTokensInMenu` (default **true**), `showCostInMenu` (default false), `showGrowthInMenu`
(default false), `showLimitInMenu` (default false), `menuLimitColorMode` (default `gauge`).

```swift
guard lastUpdated != nil else { return growthText.map { [$0] } ?? ["—"] }   // before first refresh
var usage: [String] = []
if showTokensInMenu { usage.append(TokenFormatter.compact(todayTotalTokens)) }
if showCostInMenu, showsCost { usage.append(todayUsageCost.text(l, compact: true)) }
if let growthText { usage.append(growthText) }
let limit = menuLimitLine                        // nil = hidden/unavailable
if let limit, usage.count >= 2 { return [usage.joined(separator: " · "), limit] }
if usage.count > 2 { return [usage.dropLast().joined(separator: " · "), usage.last] }
var lines = usage; if let limit { lines.append(limit) }; return lines
```

- Max 2 lines. Limit always owns its own (bottom) line. Separator is `" · "` (space, U+00B7, space).
- Empty array → only sprite (Settings hint `allOffHint`: "All off shows only the character").
- `growthText` = `companion.menuGrowthText` only when `showGrowthInMenu` (MenuBarContent.swift:5).

`menuGrowthText` (CompanionStore.swift:284-291):
`total = isEgg ? eggHatchThreshold : threshold(stage)`, `credits = isEgg ? eggUsage : usedAtStage`,
`used = clamp(credits, 0, total)`, `percent = total > 0 ? used*100/total : 0` (**integer floor**).
Text: egg → `"Hatch N%"` / ko `"부화 N%"` / ja `"孵化 N%"` / es `"Eclosión N%"` / fr `"Éclosion N%"` / pt `"Eclosão N%"` / de `"Schlüpfen N%"` / ru `"Вылупление N%"`;
otherwise `"Growth N%"` / `"성장 N%"` / `"成長 N%"` / `"Crecimiento N%"` / `"Croissance N%"` / `"Crescimento N%"` / `"Wachstum N%"` / `"Рост N%"` (Localization.swift:348-355).

Limit line (`menuLimitParts`, UsageStore.swift:392-423): only if `showLimitInMenu`; only providers with
`todayTotalTokens > 0` today; order fixed: **Claude, Codex, AGY, Cursor, OpenCode, Grok**; item text
`"<Label> <percent>"` with `TokenFormatter.percent(displayPercent)`, no "used/left" suffix; joined `" · "`.
- Claude: headline window of tracked account (5h else weekly); pace from its reset/span.
- Codex: `maxPrimaryUsedPercent`; pace from the primary window with that same percent.
- AGY (antigravity): `maxPrimaryUsedPercent`, no pace. Cursor: `planUsage.usedPercent`, no pace.
- OpenCode: rolling window, span 5h. Grok: credit utilization, span = period.
- `displayPercent = mode == .remaining ? max(0, 100 - u) : u` (UsageStore.swift:472-474).

### 3.2 Number formats (TokenFormatter.swift)

| fn | rule | examples |
|---|---|---|
| `compact(Int)` | `<1000` raw; `<999_950` → `/1e3` 1 decimal, trailing zeros/dot trimmed, `K`; `<999_950_000` → `/1e6` 1 dec `M`; else `/1e9` **2** dec `B`; sign kept | 987, 12.3K, 1K, 190.6M, 1.24B |
| `grouped(Int)` | NumberFormatter `.decimal` with **system region locale** (not app language) | 1,150,000 |
| `cost(Double)` | `"$%.2f"` | $2.76 |
| `costCompact` | tenths=`%.1f`; if <100 → `$`+tenths; else whole=`%.0f`, if <10000 → `$`+whole; else `$%.1fK` | $9.5, $311, $1.2K |
| `percent(Double)` | `%.1f`, strip `.0` | 80%, 88.3% |
| `UsageCost.text(compact:)` | unknown-only coverage → compact `"$—"` / full `l.costUnavailable` | |

Windows note: current `fmt = Intl.NumberFormat(undefined,{notation:"compact",maximumFractionDigits:1})`
(src/shared.ts) is **not** equivalent (locale-dependent suffixes, e.g. ko "120만"; B gets 1 decimal). Port `compact` literally.

### 3.3 Limit color — `PaceTier` (Core/PaceTier.swift) and `MenuLimitColoring`

```
minimumPace = 0.1
tier(u, pace, crit): pace == nil || pace < 0.1 → nil; u >= crit → wayOver;
  d = round(u - pace*100) (Int):  d < -25 wayUnder | < -10 under | < 5 onPace | < 15 slightlyOver | < 30 over | else wayOver
gauge(u, pace, warn, crit) = tier(...) ?? (u >= crit ? wayOver : u >= warn ? over : onPace)
isCalm = rawValue <= onPace
paceFraction(resetsAt, span, now) = (span - (resetsAt - now)) / span, nil if span<=0 or outside 0...1 (no clamp)
```
`menuLimitColorRuns` (UsageStore.swift:445-453): mode `off` → none; `attention` → only non-calm tiers; `gauge` → all.
Thresholds: `warnThreshold` default 80 (range 50…95), `critThreshold` default 95 (80…100), gap 5 enforced both ways.
Coloring applies `foregroundColor` to the **whole "Label NN%" substring** (first occurrence) only; separators and
token/cost/growth text keep the system menu-bar text color (MenuLimitColoring.swift:8-15).

Tier colors (`PaceTier.color`, PopoverView.swift:1950-1959) — SwiftUI system colors (hex = Apple HIG macOS values; see §8 Q1):

| tier | SwiftUI | light | dark |
|---|---|---|---|
| wayUnder | .blue | #007AFF | #0A84FF |
| under | .teal | #30B0C7 | #40C8E0 |
| onPace | .green | #28CD41 | #32D74B |
| slightlyOver | .yellow | #FFCC00 | #FFD60A |
| over | .orange | #FF9500 | #FF9F0A |
| wayOver | .red | #FF3B30 | #FF453A |

`percentColor` (popover % text) = `.primary` for slightlyOver (yellow unreadable on light), else tier color.

### 3.4 Status item geometry & text (PokeTokenBarApp.swift)

- Button: `imagePosition = .imageLeading`, font `monospacedDigitSystemFont(13, .regular)`, multi-line allowed (:119-121).
- Sprite canvas: height **22pt** fixed; sprite fitted (aspect kept, nearest-neighbor) into **width ≤ 36pt × height 20pt**;
  canvas width = fitted width + 2 (1pt each side); drawn at x=1, y = 0 (frame A) or 1 (bob up) — bottom aligned (:513, :535-550).
- Egg glyph: 22×22 canvas, "🥚" system font 15pt drawn in rect (2, off, 20, 20), off 0/1 (:564-574).
- 1 line: title = `" " + text` (leading space) in 13pt; colored via attributedTitle if runs exist (:253-266).
- ≥2 lines: `thickness` = menu bar height (22 classic, 24 on notched Macs);
  `share = thickness / lines`; `fontSize = min(11, max(8, floor(share*0.85)))`; `lineHeight = min(share, round(fontSize*1.28))`;
  centered paragraph, monospaced digits; baseline offset measured so the text block is vertically centered (:221-252).
  → 22pt bar: 9pt font, 11pt line height; 24pt bar: 10pt font, 12pt line height.
- No dimming ever (`appearsDisabled = false`, :208).
- Text color: system (white on dark menu bar, black on light) except limit runs.

Screenshot check: `assets/screenshot-menu-growth.png` (200×44 px @2x) shows sprite + two centered lines "1.2M" / "Growth 42%";
`assets/menubar.gif` shows sprite + single line "34.9M".

### 3.5 Sprite animation (PokeTokenBarApp.swift:289-506, SpriteAnimation.swift)

- `AnimationQuality.frameFloor`: powerSaver **0.4 s** (default), balanced 0.2, smooth 0.1. Effective floor with
  OS low-power = `max(floor, 0.4)`.
- Fallback "bob": 2 frames, 0.5 s each (static sprite y=0, y=1). Egg: same with egg glyph.
- Animated: Gen-V GIF frames → `capFrameRate(frames, floor)` → composited to 22pt canvas.
- `capFrameRate`: accumulate delays; when `acc + 1e-9 >= floor` emit (first frame of bucket, acc); leftover tail
  added to last emitted frame's delay (or emitted alone if none). Preserves loop length; never stretches.
- GIF delay: unclamped delay → clamped → 0.1; if `< 0.02` → **0.1** (SpriteAnimation.swift:56-63). Single-frame GIF → [] (use static).
- Timer tolerance = 10% of frame delay (:309).
- Cache key: `"\(id)-\(shiny)-\(floor)-\(form ?? "")"` + `"-back"` if back (:501-506). Changing floor/species/shiny/form/back rebuilds.
- Load order: PNG cache → bob immediately (or egg if no PNG); then GIF; if GIF unavailable and no cached PNG, download PNG → bob.
- Pauses: display asleep, popover shown, status window not visible (occlusion). Resumes on the inverse.

### 3.6 Status menu (StatusItemMenu.swift)

Order (nil = separator):

| # | title (en) | key equiv | action | notes |
|---|---|---|---|---|
| 1 | `"Today's tokens 19.4M · $5.09"` | — | none (disabled, grey) | `"\(l.todayTokens) \(compact(todayTotal))"` + `" · " + cost.text(l)` only if `showsCost` |
| — | separator | | | |
| 2 | Refresh now | ⌘R | refresh | |
| 3 | Pokédex | ⌘D | openDex | popover → Collection tab, Pokédex segment |
| 4 | Settings… | ⌘, | openSettings | popover with Settings shown |
| 5 | Hide floating pet / Show floating pet | — | toggleFloatingPet | label by current state |
| 6 | Show back view | — | toggleBackView | checkmark = `alwaysFacesBack` |
| — | separator | | | |
| 7 | Quit | ⌘Q | quit | |

Localized strings (ko/en/ja): todayTokens 오늘 사용한 토큰 / Today's tokens / 本日のトークン; refreshNow 지금 새로고침 / Refresh now / 今すぐ更新;
dexTitle 도감 / Pokédex / 図鑑; settings 설정 / Settings / 設定 (+ "…"); floatingPetHideLabel 플로팅 펫 숨기기 / Hide floating pet / フローティングペットを隠す;
floatingPetEnableLabel 플로팅 펫 표시 / Show floating pet / フローティングペットを表示; backViewLabel 뒷모습 보기 / Show back view / 後ろ姿を見る;
quit 종료 / Quit / 終了. (es/fr/pt/de/ru in Localization.swift:24-1868.)
Menu is built fresh per right-click, attached only for that click, then detached so left click keeps toggling the popover (:601-613).
Opening the menu closes the popover first.

### 3.7 Popover metrics (PopoverView.swift:8-19)

`width 360`, `padding 14`, `contentWidth 332`, `scrollerInset 12` (trailing padding always reserved inside vertical
ScrollViews), `scrollContentWidth 320`. Main VStack spacing **12**. Tab bodies fixed height **520**
(Home ScrollView :184, ShopView.swift:31, BagView.swift:22, CompanionView.swift:20/1068). Settings view height **460**
(SettingsView.swift:107), no outer padding. Measured popover (screenshot-home.png 720×1208 @2x): **360×604pt**.

---

## 4. Data / persistence

UserDefaults keys read by the shell (UsageStore.swift:952-981):

| key | type | default |
|---|---|---|
| `showTokensInMenu` | Bool | true |
| `showCostInMenu` | Bool | false |
| `showGrowthInMenu` | Bool | false |
| `showLimitInMenu` | Bool | false |
| `menuLimitColorMode` | String `gauge\|attention\|off` | gauge |
| `limitDisplayMode` | String `used\|remaining` | used |
| `warnThreshold` / `critThreshold` | Double | 80 / 95 (warn ≤ crit−5) |
| `animationQuality` | String `powerSaver\|balanced\|smooth` | powerSaver |
| `floatingPetEnabled` | Bool | false |
| `alwaysFacesBack` | Bool | false |
| `updateNotificationsEnabled` | Bool | true |
| `refreshInterval` | seconds | 120 (0 = manual) |
| `NSInitialToolTipDelay` | registered default | 600 ms |

Not persisted: `PopoverNavigation` (`tab`, `showSettings`, `showingRecap`, `showingCollectionLog`,
`dexDetailCollectionID`, `providerID`, `claudeAccountID`, `expandAdvancedOnOpen`) — in memory for the process lifetime;
`reset()` clears `showSettings`, `expandAdvancedOnOpen`, `showingRecap`, sets `tab = .home` (not providerID/claudeAccountID/showingCollectionLog).

Localized PokéAPI names (PokemonNameLabel / PokemonNameClient): `GET https://pokeapi.co/api/v2/{kind}/{name}/`,
kind ∈ `type|ability|move|pokemon-form`; name must match `[a-z0-9-]+`; cached per resource as
`<AppState>/pokemon-names-v1/<kind>-<name>.json` for **30 days**. Resolution: app-language API codes
(ko `ko`, en `en`, ja `ja-hrkt`→`ja`, es, fr, pt `pt-br`→`pt`, de, ru) then `en`; keys lower-cased/trimmed.

Legacy migration (macOS only): `~/Library/Application Support/TokenMac` → `PokeTokenBar` if new missing (:554-561). Not needed on Windows.

---

## 5. UI spec

### 5.1 Color tokens (light / dark)

Values: macOS semantic colors resolved for an NSPopover (vibrant material) plus samples from `screenshot-home.png`
and `settings.png` (both dark). Light values are Apple's documented defaults; popover material is approximated as flat.

| token | macOS source | light | dark (sampled) |
|---|---|---|---|
| `--popover-bg` | NSPopover material | #ECECEC | #2C2C2C |
| `--settings-bg` | windowBackgroundColor | #ECECEC | #1E1E1E |
| `--card-bg` (settings group) | controlBackgroundColor | #FFFFFF | #1E1E1E (border carries it) |
| `--card-border` | separatorColor ×0.6, 1px | rgba(0,0,0,.06) | #343434 |
| `--separator` (Divider) | separatorColor | rgba(0,0,0,.10) ≈ #D9D9D9 on bg | #414141 |
| `--text` (.primary) | labelColor | rgba(0,0,0,.85) | rgba(255,255,255,.85) ≈ #DADADA (big number sampled #CACACA AA) |
| `--text-2` (.secondary) | secondaryLabelColor | rgba(0,0,0,.50) | rgba(255,255,255,.55) ≈ #8E8E8E |
| `--text-3` (.tertiary) | tertiaryLabelColor | rgba(0,0,0,.26) | rgba(255,255,255,.25) ≈ #5E5E5E–#6A6A6A |
| `--accent` | controlAccentColor (default blue) | #007AFF | #0A84FF (sampled #007AFF text) |
| `--seg-track` | segmented control bg | #E3E3E3 | #3C3C3C |
| `--seg-selected` | selected segment | #FFFFFF + 0 0.5px 1px rgba(0,0,0,.15) | #555555 |
| `--seg-divider` | between unselected segments | #C8C8C8 | #4F4F4F |
| `--fill-8` | `Color.primary.opacity(0.08)` (recap capsule) | rgba(0,0,0,.08) | rgba(255,255,255,.08) ≈ #3A3A3A |
| `--chip` | `Color.secondary.opacity(0.08)` | rgba(0,0,0,.04) | ≈ #353535 |
| `--chip-selected-bg` | accent ×0.18 | rgba(0,122,255,.18) | ≈ #243A52 |
| `--icon` (footer, borderless buttons) | secondaryLabel-ish | rgba(0,0,0,.50) | #989898 |
| `--track` (progress) | system progress track | rgba(0,0,0,.10) | #434343 |
| orange (egg progress / warning) | .orange | #FF9500 | #FF9F0A (sampled #FF9230) |
| `--banner-update-bg` | accent ×0.12 | rgba(0,122,255,.12) | rgba(10,132,255,.12) |
| `--banner-warn-bg` | orange ×0.12 | rgba(255,149,0,.12) | rgba(255,159,10,.12) |
| `--gray` | .gray | #8E8E93 | #8E8E93 |

### 5.2 Typography (macOS SwiftUI text styles → px at 100%)

Font: SF Pro → Windows `"Segoe UI Variable Text", "Segoe UI", system-ui`; `.monospacedDigit()` → `font-variant-numeric: tabular-nums`.

| style | size/weight | used for |
|---|---|---|
| `.system(28, .bold)` | 28 bold | today's total |
| `.headline` | 13 semibold | Settings title |
| body (default) | 13 regular | segmented tabs, settings rows, menu bar 1-line (13) |
| `.callout` | 12 | provider row, today cost |
| `.caption` | 10 | captions, footer "Updated", capsule tabs, update banner |
| `.caption2` | 10 (lighter use) | hints, footer links |
| Badge | `.system(8, .bold)` default | type/rarity pills |

(macOS caption/caption2 are both 10pt; Windows may round to 11px for legibility — see §8 Q2.)

### 5.3 Popover component tree (main content)

```
Popover 360 wide, background --popover-bg, arrow at top pointing to status item
└ VStack(alignment: leading, spacing 12) padding 14
  ├ UpdateBanner?                       (§5.6)
  ├ SegmentedPicker [Home | Shop | Bag | Collection]   full width 332, ~24pt high, labels 13pt
  ├ Tab body (height 520)
  │   Home: ScrollView { VStack(spacing 12) { CompanionHeader · Divider · Header · Divider ·
  │         ProviderStatusBanner? · [LimitsSection · Divider]? } .padding(trailing 12) }
  │   Shop: ShopView   Bag: BagView   Collection: CollectionView (own segmented Pokédex/Catch log)
  └ Footer (§5.4)
```
Screenshot measurements (dark, @1x pt): segmented control x 14…342, ~24pt tall, corner radius ~6; selected segment fill #555555,
unselected segments separated by 1px #4F4F4F dividers ~14pt tall; Divider lines #414141 1px full content width;
footer icons baseline at y≈586 of 604.

Segmented picker: macOS `.pickerStyle(.segmented)`, equal-width segments, label 13pt regular, selected label primary,
no icons. Changing `tab` away from `.collection` clears `dexDetailCollectionID`.

### 5.4 Footer (PopoverView.swift:1105-1157)

`HStack(spacing: 10)`, vertically centered, no background, no top divider.

Left group — **only when `tab == .home`**:
1. Refresh button: SF Symbol `arrow.clockwise`, `.borderless` (icon only, no chrome, tinted secondary), tooltip `l.refreshNow`.
   Action `store.refresh()`; **no spinner** (re-entrancy guarded in store; feedback is the timestamp).
2. If `lastUpdated != nil`: `Text("\(l.updated) ") + Text(updated, style: .relative)` — caption, `.tertiary`. Live ticking
   relative duration without "ago": en "Updated 0 sec", "Updated 2 min, 5 sec", "Updated 1 hr, 3 min"; ko "갱신 2분 5초"; ja "更新 2分5秒".
3. If `lastErrorDescription != nil`: SF `exclamationmark.triangle`, `.orange`, tooltip `usageRefreshError + "\n" + description`
   (en "Some usage could not be loaded. Please refresh again.").

`Spacer()`

Right group — **all tabs**:
4. Pet toggle: `eye` when `floatingPetEnabled` else `eye.slash`; tooltip + a11y label: enabled → "Hide floating pet", disabled → "Show floating pet". Toggles `floatingPetEnabled`.
5. Settings: `gearshape`, tooltip `l.settings`, sets `nav.showSettings = true`.
6. Quit: `power`, tooltip `l.quit`, terminates the app.

Icons ~13–15pt glyphs (screenshot: ~13pt, color #989898 dark). Borderless button hover: no background on macOS (only pressed dim).

### 5.5 Settings shell (SettingsView.swift:65-147, 938-980)

Replaces the whole popover content (no outer 14 padding); frame 360×460.
- Header: `HStack(spacing 6)` padding h14 v10: `[‹ chevron.backward + l.back]` (plain button, accent color, **Esc** = cancelAction) · Spacer · title `l.settings` `.headline` · Spacer · invisible copy of "Back" for centering.
- Divider; ScrollView with VStack(spacing 18) padding 16 of groups; Divider; footer.
- Footer: `HStack(spacing 5)` caption2 `.tertiary`, padding h16 v10: `v<version> · GitHub · Website · ♥ Sponsor` — links underlined plain buttons, tooltip = URL:
  `https://github.com/chattymin/PokeTokenBar`, `https://chattymin.github.io/PokeTokenBar/`, `https://github.com/sponsors/chattymin` (Windows port should point at its own repo; see §8 Q5).
- Group section: title caption semibold `.secondary` UPPERCASE, leading pad 4, spacing 6; card bg controlBackground, radius **8**, 1px stroke separator×0.6.
- Row: `HStack(spacing 10)` padding h12 v8, minHeight **38**; rows separated by Divider; toggles are `.switch` small (macOS switch ~32×18).
- Close (`onClose`) sets `showSettings=false`, `expandAdvancedOnOpen=false` → back to the tab that was showing.
- `openSessionKeySettings()` opens with Advanced expanded and scrolls to it after 80 ms, focusing the session key field.

### 5.6 Update banner (PopoverView.swift:123-147)

Shown when `updater.available != nil && store.updateNotificationsEnabled`. VStack(leading, spacing 6), padding 8,
bg accent×0.12, radius 8. Text caption: `"🆕 v{new} available (you have {current})"` (ko `"🆕 v{new} 사용 가능 (현재 {current})"`).
Row (spacing 8, right aligned): when updating → `"Updating…"` caption2 secondary + small spinner; else
`"Skip this version"` (borderless, small, secondary) + `"Update"` (borderedProminent = filled accent, white text, small).

### 5.7 Shared components

**Badge** (Badge.swift): capsule; font `system(size, .bold)` size default **8**; padding h **5** v **1**;
`.solid` → white text on tint; `.tinted` → tint text on tint×0.14. Type badges add text shadow `black 35%, radius 0.5, y 0.5`.
TypeBadges: HStack spacing 5, label = localized type name uppercased.
Type colors: normal #AAAA99, fire #FF4422, water #3399FF, electric #FFCC33, grass #77CC55, ice #66CCFF, fighting #BB5544,
poison #AA5599, ground #DDBB55, flying #8899FF, psychic #FF5599, bug #AABB22, rock #BBAA66, ghost #6666BB, dragon #7766EE,
dark #775544, steel #AAAABB, fairy #EE99EE; unknown type (stellar/shadow/unknown) → .gray.
Rarity tint (CompanionView.swift:3-10): uncommon .green, rare .blue, legendary .orange, common/nil .gray (system hexes §3.3/§5.1).
Other tints: accent, orange, secondary.

**PokemonNameLabel**: text = items joined `" · "`; per item: translated name for app language → else identifier;
while loading `"…"`; on failure `identifier(raw)` = split on `-`, capitalize first letter of each, join with space
("pokemon-form" → "Pokemon Form"); suffix appended. Fetch only when mounted; language change re-resolves without refetch.

**CapsuleTabBar** (provider tabs, Claude account tabs): horizontal scroll (no indicators, bounce only if overflow),
HStack spacing 6; item = HStack(spacing 4)[optional 5×5 dot, title one line]; font caption, semibold when selected;
padding h10 v4; capsule bg selected accent×0.18 / else secondary×0.08; text accent / secondary; tooltip optional.

**LimitProgressBar**: small ProgressView (track 6pt) filled `displayPercent` clamped 0…100 in tier color; pace marker
2.5×10pt (track 6 + 2 overhang top/bottom), radius 1.25, `primary × 0.55`, x = `(width − 2.5) × markerFraction`,
`markerFraction = displayPercent(pace×100)/100` (flips with remaining mode), no hit testing.

Recap button: HStack(3)[`chart.bar.xaxis`, caption text] padding h8 v4, bg primary×0.08 capsule, plain.

### 5.8 States

- Before first refresh: menu bar `"—"` (or growth text only); footer has no "Updated".
- Refresh error: orange triangle in footer (Home only); menu bar unchanged, never greyed.
- No usage today: tokens line shows `0`; limit items omitted for providers unused today.
- Popover hover states: macOS borderless/plain buttons have no hover fill; tooltips after 600 ms.

---

## 6. Windows adaptation

| macOS mechanism | Windows / Tauri equivalent |
|---|---|
| `NSStatusItem` with sprite + text title | `TrayIconBuilder` icon only. Windows cannot draw text in the notification area. Render the **sprite frame** as the tray icon (RGBA via `tauri::image::Image::new_owned`, 32×32 px canvas, nearest-neighbour, bottom-aligned, transparent; Windows downsamples for 16/20/24 at 100–150%). Text goes to the tooltip. |
| Status item text lines + tier colors | Tooltip `set_tooltip`: `"PokeTokenBar\n" + lines.join("\n")` (+ `"\nTracked Claude account: X"` when applicable). Windows tooltip ≤127 chars, plain text, no colors → limit coloring cannot apply; optionally prefix non-calm items with ▲ (open question). Keep `set_title` no-op on Windows. |
| Menu bar sprite animation (timer, frame floor, pause rules) | Rust timer thread calling `tray.set_icon()` with pre-rendered frames at `effectiveFrameFloor`; pause when popover visible, when session locked/display off (`WM_POWERBROADCAST`/`WTSRegisterSessionNotification`), and on battery saver (`GetSystemPowerStatus().SystemStatusFlag == 1` ↔ macOS Low Power → cap 0.4 s). Reuse `capFrameRate` + delay rules verbatim (decode GIF in Rust `image`/`gif` crate). |
| `NSPopover` (.transient, arrow, anchored under item) | Main webview window: `decorations:false`, `resizable:false`, `skipTaskbar:true`, `alwaysOnTop:true`, `shadow:true` (Win11 rounds corners), `visible:false` at start, size 360×604 logical (360×460 for Settings). Position from `TrayIconEvent::Click { rect }`: centre on icon x, clamp to monitor work area with 8px margin, place above taskbar (bottom/top/left/right edge detection via work area vs monitor bounds). No arrow. |
| Transient close + global outside-click monitor | `WindowEvent::Focused(false)` → hide. Guard: ignore a tray left-click toggle within ~250 ms after a blur-hide (otherwise click on tray re-opens). |
| `NSApp.activate` before show | `window.show()` + `set_focus()`. |
| Content built on open / released on close | Not needed for CPU; but must emit a `popover-opened` event so the frontend runs `nav.reset()` (Home, Settings closed, Recap closed) and the Rust side pauses tray animation + runs debounced update check. |
| Right click / Ctrl+click → NSMenu built per click | Tray `.menu(&menu)` + `show_menu_on_left_click(false)`; Windows shows on right click. Keep `MenuItem` handles and `set_text` the summary row and pet label on every refresh/settings change; back view as `CheckMenuItem`. Accelerator text `Ctrl+R`, `Ctrl+D`, `Ctrl+,`, `Ctrl+Q` (display only; optionally real shortcuts inside the popover webview). Ctrl+left click → also menu (call `tray.menu popup` manually — optional). |
| `.accessory` activation policy | `skipTaskbar:true` on all windows; no main-window on launch. |
| `SingleInstance.shouldYieldToRunningInstance()` (later instance quits silently) | `tauri_plugin_single_instance` (already). macOS second launch shows nothing; Windows currently reveals main — acceptable, or match by doing nothing. |
| `LoginItem` / LaunchAgent (KeepAlive) | `tauri_plugin_autostart` (HKCU Run key) — already wired; KeepAlive auto-restart has no equivalent (optional: Task Scheduler "restart on failure"). |
| `NSInitialToolTipDelay` 600 ms | Custom tooltip component in webview (CSS/JS, 600 ms delay) — native `title` delay is not controllable. |
| URL scheme `poketokenbar://agent…` (Apple Event GetURL) | Register `poketokenbar` protocol via `tauri-plugin-deep-link` (HKCU\Software\Classes\poketokenbar); deliver via single-instance args; must not show/focus any window. |
| UNUserNotification authorization on first open | No-op on Windows (toast needs AppUserModelID only; `tauri-plugin-notification`). |
| NSWorkspace screensDidSleep/Wake, occlusion | `WM_POWERBROADCAST` (PBT_APMSUSPEND/RESUMEAUTOMATIC), `WTS_SESSION_LOCK/UNLOCK`; occlusion: skip (tray always "visible"). |
| SF Symbols (arrow.clockwise, exclamationmark.triangle, eye, eye.slash, gearshape, power, chevron.backward, chart.bar.xaxis) | Segoe Fluent Icons glyphs: Refresh `E72C`, Warning `E7BA`, View `E890`, Hide `ED1A`, Settings `E713`, PowerButton `E7E8`, ChevronLeft `E76B`, BarChart `E9D2`; or inline SVGs. |
| System semantic colors / light-dark | CSS tokens in §5.1 under `:root` + `@media (prefers-color-scheme: dark)`; WebView2 follows Windows app theme. Accent: may read Windows accent via `UISettings` but default #007AFF/#0A84FF matches screenshots. |
| `.help()` tooltips, Text(style:.relative) | Tooltip component; `Intl.DurationFormat(lang,{style:"short"})` on the elapsed seconds, re-rendered every 1 s while visible, largest two units. |
| Floating pet ↔ label | macOS pet does not show menu-bar text. Windows has no visible label at all, so propose (opt-in) a small caption under the pet with the same `menuLines` and tier colors (§8 Q3). Pet click = `openPopover()` (show only, never toggle off). |

---

## 7. Gaps vs current Windows code

Current code: `src/App.tsx` (tabs home/dex/settings + ↻ in nav), `src/App.css`, `src-tauri/src/lib.rs`, `src-tauri/src/settings.rs`, `src-tauri/tauri.conf.json`.

1. **Tabs**: Windows has `Home | Dex | Settings` + ↻ button; macOS has segmented `Home | Shop | Bag | Collection`, Settings via footer gear. Shop and Bag do not exist.
2. **Footer** missing entirely (refresh, Updated relative time, error triangle, pet eye toggle, gear, quit).
3. **Window chrome/geometry**: `main` is a decorated, centered 360×680 window, `visible:true` at launch; macOS: undecorated 360×604 popover positioned at the tray, hidden at launch, hides on blur. No Settings 360×460 size. `tauri_plugin_window_state` restores position — conflicts with tray anchoring.
4. **Hide on blur / reset to Home on open**: absent (tab state persists; window only toggles).
5. **Tray icon** is the static app icon; no sprite, no animation, no egg, no frame floor/animation quality setting, no pause rules.
6. **Tray label/tooltip**: `update_tray` builds `"PokeTokenBar · today 1.2M · limit 80%"` from `tray_tokens`/`tray_limit` only. Missing: cost item, growth item, per-provider limit items (Claude/Codex/AGY/…) with `Label NN%`, only-used-today filter, 2-line combination rule, `"—"` before first refresh, `TokenFormatter.percent` formatting (uses `{:.0}`), tracked-account tooltip. Uses a single `peak` across providers instead of per-provider items.
7. **Settings keys**: missing `showCostInMenu`, `showGrowthInMenu`, `menuLimitColorMode`, `animationQuality`, `alwaysFacesBack`, `updateNotificationsEnabled`. `refresh_minutes` default 1 (macOS 120 s = 2 min). `pet_enabled` default true (macOS `floatingPetEnabled` false). Languages 3 (ko/en/ja) vs 8.
8. **Tray menu**: only `Refresh / Show/Hide Pet / Quit`, hardcoded English. Missing summary row (disabled), Pokédex, Settings…, state-dependent pet label, Show back view (checked), separators, accelerators, localization.
9. **Number formatting**: `Intl.NumberFormat` compact ≠ `TokenFormatter.compact` (§3.2); no `grouped`, `costCompact`, `percent`.
10. **Visual language**: GitHub-like palette (`--accent #2da44e` green, `--bg #0d1117`), 12px cards with 14px padding, toasts in accent; macOS uses neutral popover grey (#2C2C2C dark), dividers instead of cards on Home, system blue accent, orange progress, 8pt radius cards only in Settings. Fonts 13/20px vs 10/12/13/28 scale.
11. **Update banner**: Windows shows only a red dot on the Settings tab; macOS shows an in-content banner with Skip / Update and "Updating…" state. `skipCurrent` (skip this version) missing.
12. **Limit tier colors**: Windows uses 3 fixed colors (`warn #bf8700` ≥80, `danger #cf222e` ≥95, else green) hardcoded 80/95 instead of user thresholds and 6-tier pace colors; no pace marker.
13. **Shared components**: no `Badge` spec match (Windows `.badge` radius 6, 12px, GitHub colors; rarity colors differ: uncommon #1f883d, rare #0969da, legendary #bf8700 vs system green/blue/orange), no type badges, no `PokemonNameLabel` (localized PokéAPI names for type/ability/move/form with 30-day cache), no `CapsuleTabBar`.
14. **Back-view sprite** (`alwaysFacesBack`) and **forms** not supported in sprite URLs (`sprite(id, shiny)` only).
15. **Deep link** `poketokenbar://agent` not registered (no `tauri-plugin-deep-link`).
16. **Toast for companion events** in popover (6 s) has no macOS counterpart in the shell (macOS uses notifications + floating pet). Keep or drop — §8 Q4.
17. `MacosLauncher::LaunchAgent` / `macOSPrivateApi` are mac-only leftovers; harmless.

---

## 8. Open questions

1. Exact system color hexes: macOS 26 renders slightly different system colors (sampled dark orange #FF9230 vs HIG #FF9F0A; teal values differ between macOS releases). Use the HIG table above or sample a light-mode screenshot? No light-mode screenshots exist in `assets/`.
2. macOS `.caption` is 10pt; at 100% Windows scaling 10px Segoe UI is hard to read. Keep 10px for pixel parity or use 11px?
3. Windows has no visible menu-bar text. Add an opt-in caption under the floating pet showing `menuLines` with tier colors, or rely on the tooltip only?
4. Keep the Windows-only in-popover toast for hatch/evolve events?
5. Settings footer links: keep upstream URLs (chattymin) or point to the Windows fork (`tkdals2317/PokeTokenBar-Windows`)? Sponsor link?
6. Tray icon size: render at 32 px (crisp at 200%) and let Windows downscale, or provide per-DPI 16/20/24/32 frames (pixel-art downscaling of 36×20 sprites to 16 px is lossy)?
7. Popover anchor when the taskbar is auto-hidden or on a secondary monitor — follow the tray rect monitor (recommended) or the cursor monitor?
