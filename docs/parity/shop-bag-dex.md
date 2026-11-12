# Parity spec: Shop, Bag, Items, Pokédex (Collection)

Source of truth: PokeTokenBar v2.5.7 (macOS, Swift), files under `Sources/PokeTokenBar/`.
Target: this repo (Tauri 2, `src-tauri/src/*.rs` + `src/*.tsx`).
Cites are `File.swift:line` in v2.5.7.

---

## 1. Overview

The macOS popover has four tabs: `home, shop, bag, collection` (`PopoverView.swift:4`), plus Settings
and Recap overlays. This spec covers:

- **Economy**: tokens used since install are a wallet. Spendable = `usedSinceInstall − spentTokens`.
  Buying never touches growth or statistics; it only raises `spentTokens`.
- **Items** (`ItemKind`): Rare Candy (consumable XP), Mint (consumable nature reroll), Shiny Charm
  (passive, one-time buy).
- **Eggs** (shop rows, not items): reroll the current Pokémon into a new egg, in three tiers
  (no guarantee / Uncommon-or-better / Rare-or-better).
- **Free Rare Candy grants** when a provider limit window reaches 100%, edge-triggered and
  deduplicated per window with epoch (reset) tracking.
- **Bag**: owned items, use flows (bulk candy with growth preview, mint).
- **Collection tab**: Pokédex (species grid 4×4 paged) and Catch log (one row per individual),
  shared search / shiny / sort toolbar, rarity and region chips with counts, completion badges and
  notifications, Pokémon entry detail page (forms, appearance, individual picker, profile, stats,
  moves), Back that returns to the same page/row.

The Windows port today has **none of shop/bag/items/candy/regions/badges**, and a reduced Pokédex
(see §7).

---

## 2. Features (numbered)

1. Wallet header in Shop: "Spendable tokens" + compact amount (24pt bold) + hint line.
2. Shop list = items + 3 egg tiers merged and sorted by **effective price ascending**; a purchased
   passive item sinks to the bottom.
3. Item purchase with inline confirm (button morphs; no modal), quantity stepper for consumables
   (1…max affordable), price line shows total for the chosen quantity.
4. Passive item (Shiny Charm) is one-time: after purchase the card shows "✓ Owned" instead of a buy
   control; Shiny Charm changes shiny odds 1/64 → 1/48 for future hatches.
5. Egg purchase: releases the active Pokémon (kept in the Pokédex as a *Released* record), resets
   egg incubation, stores the rarity guarantee; inline 1- or 2-step confirm (2-step for shiny or
   legendary); after success switch to Home.
6. Egg cards stay visible during the egg stage with a disabled Buy and the hint
   "Available once your current egg hatches."
7. Shop difficulty multiplier (0.1–2.0, default 1.0) scales all shop prices.
8. Free Rare Candy: +1 when a session (≈5h) window first reaches 100%, +5 for a weekly window;
   one notification per grant; first-run seed prevents retroactive grants; epoch rearm (#326) with
   minimum advance 30 min / 24 h (#375, v2.5.6); Codex windows carry no epoch and rearm only after a
   sub-100% sample (#434, v2.5.7).
9. Bag: owned items (count > 0) in `ItemKind.allCases` order; empty state = animated Snorlax + "Your
   bag is empty!" + Shop button.
10. Rare Candy bulk use: stepper 1…`maxRareCandyUseCount` (never more than needed to graduate),
    preview lines (carry-over XP after evolution / "Expected to graduate." / discarded XP warning).
11. Mint use: nature becomes a random *different* nature; Home plays a sparkle.
12. Use → switches to Home; candy shows "+100M XP" floating text for 1.3 s.
13. Collection segmented control: Pokédex | Catch log (segment remembered while popover lives).
14. Shared filter toolbar: search field with clear button, shiny-only toggle, sort menu (accent
    highlighted when not default).
15. Pokédex header: title + "N species" + earned national/Unown medals; region chips with
    `owned/total` and 🏅/✨; rarity chips with species counts.
16. Pokédex grid: 4 columns × 4 rows = 16 per page; cells show `#id` (rarity-colored), static
    sprite (shiny if a shiny was owned and the species has no forms), name (+ `n/total` forms),
    ✨ marker, RAISING badge, ★ for representative; hover scale + shadow; right-click menu.
17. Paging: chevrons with 28×28 hit area, "1 / 2", scroll wheel / trackpad paging; any filter change
    resets to page 0.
18. Footer: selected cell's "#id name · Rarity" + representative button (species without forms).
19. Catch log: one card per individual (active first, then newest), rarity badge, RAISING or
    RELEASED badge, ✨, nature, evolution chain sprites with names, relative time.
20. Detail page (replaces the grid inside the tab): Back, `#id`, form picker, Normal/Shiny segmented
    picker, identity header (104pt animated sprite, name, rarity, type badges, ✨ Shiny, Raising,
    representative button), individual menu (only if >1), individual card, species data, full move
    list; loading / error+Retry states.
21. Back returns to the grid page that contains the species, or scrolls the catch log back to the
    row that opened it.
22. Region completion badges (5 regions × normal/shiny, Unown 28 forms, National, National shiny):
    one-time notification each, first-run seed without notifications.

---

## 3. Rules & constants

### 3.1 Items (`CompanionModel.swift:182-276`)

| ItemKind (rawValue) | sprite name | fallback emoji | base price | passive | effect |
|---|---|---|---|---|---|
| `rareCandy` | `rare-candy` | 🍬 | 500,000,000 | no | +100,000,000 XP to `usedAtStage` |
| `mint` | none (nil) | 🌿 | 100,000,000 | no | nature → random different nature |
| `shinyCharm` | `shiny-charm` | ✨ | 3,000,000,000 | yes | shiny denominator 64 → 48 |

- Item sprite URL: `https://raw.githubusercontent.com/PokeAPI/sprites/master/sprites/items/{name}.png`
  (`SpriteLoader.swift:7`). Draw with nearest-neighbour, aspect-fit into a 30×30 box; emoji at
  `font-size = box` while loading or on failure (`CompanionView.swift:18-49`).
- `RareCandy.xp = 100_000_000`, `RareCandy.weeklyGrant = 5`, `RareCandy.price = 500_000_000`
  (`CompanionModel.swift:221-233`). Candy XP is **not** scaled by growth difficulty.
- `Mint.price = 100_000_000` (`:240`). `ShinyCharm.price = 3_000_000_000`,
  `ShinyCharm.shinyDenominator = 48` (`:246-249`); base `PokemonOdds.shinyDenominator = 64` (`:446`).
  Shiny roll: `roll % (charmOwned ? 48 : 64) == 0` (`CompanionStore.swift:1726-1736`).

### 3.2 Eggs (`CompanionModel.swift:253-276`)

- `FreshEgg.price = 1_000_000_000`; `shopTiers = [nil, .uncommon, .rare]` (legendary never sold).
- Tier price = `round(1B × graduationTotal(tier) / graduationTotal(.common))`:
  graduation totals common 750M, uncommon 1.875B, rare 3B, legendary 6B → **1B / 2.5B / 4B**.
- Guarantee = capture-rate ceiling: rare ≤ 45, uncommon ≤ 120, common ≤ 255, legendary nil
  (`:79-86`). `Rarity.includes(captureRate:)` = `captureRate <= ceiling`.
- Hatch candidate filter for a guaranteed egg: base index filtered by `tier.includes(captureRate)`;
  empty → keep egg, retry next tick (no fallback to full pool) (`CompanionStore.swift:1912-1940`).
  REST fallback applies the same filter, 16 tries (`:1942-1965`).
- Final gate after line fetch: if `line.rarity.sortRank < tier.sortRank` discard the pre-roll and
  retry next tick, egg kept (`:1754-1757`). sortRank: common 0, uncommon 1, rare 2, legendary 3.
- `eggTier` is consumed (set nil) on hatch. `eggGuarantee` (UI) = `eggTier` only while there is no
  active Pokémon; Home shows badge "`{Rarity}` or better" tinted by rarity (`CompanionView.swift:792`).

### 3.3 Prices and shop difficulty

- `price(of:) = round(basePrice × clamp(shopDifficulty))` (`CompanionStore.swift:174-182`,
  `CompanionModel.swift:150-152`). Clamp: non-finite → 1.0; else clamp to `0.1…2.0`.
- Stored in UserDefaults key `shopDifficulty` (Double, default 1.0) — not in the save file
  (`CompanionStore.swift:63-99`). Changing it has no state side effects.

### 3.4 Wallet and purchase rules (`CompanionStore.swift:1273-1417`)

```swift
var availableTokens: Int { max(0, state.usedSinceInstall - state.spentTokens) }
func canBuy(_ kind) -> Bool  // price != nil && !(passive && owned) && available >= price
func maxBuyCount(_ kind) -> Int  // canBuy ? (passive ? 1 : available / price) : 0
func buy(_ kind, count) -> Bool  // 1 <= count <= maxBuyCount; spent += price*count; inventory[kind] += count; save
```

- No partial buys. Items can be bought in the egg stage (stockpiling allowed).
- `purchasableItems` sort: purchased passives last; otherwise item base price ascending.
- `shopEntries` sort: purchased passive last; otherwise **effective price** (`price(of: entry)`)
  ascending. Default order at difficulty 1.0: Mint 100M, Rare Candy 500M, Pokémon Egg 1B,
  Uncommon Egg 2.5B, Shiny Charm 3B, Rare Egg 4B. After buying the Charm it moves to the end.
  Swift `sorted` is not stable-guaranteed; ties do not occur with default prices.
- `canBuyEgg(tier)`: `FreshEgg.shopTiers.contains(tier) && hasActive && available >= price`.
- `buyEgg(tier)` (`:1378-1403`), in order:
  1. `spentTokens += price`
  2. append `releasedDexEntry(active)`: id = `profile.instanceID ?? UUID`, `chainOrder =
     pathIDs.prefix(max(1, stageIndex+1))` (reached forms only), `finalID = last reached`,
     `caughtAt = releasedAt = now`, `isShiny = currentIsShiny` (Ditto disguise hides shiny), nature,
     profile, names for the reached chain, form, prayers (`:355-381`).
  3. `active = nil`; reconcile representative; `collectedFinals` **unchanged** (release ≠ graduate,
     hatch weight of the released base stays 100%).
  4. `eggUsage = 0`, egg prayers reset, `eggTier = tier`, `pendingHatchID = nil`,
     `pendingForm = nil`, clear event banners; start prefetch; save.
- High-value check for 2-step confirm: `currentIsShiny || rarity == .legendary` (`:193-195`).

### 3.5 Bag usage

- `ownedItems`: `ItemKind.allCases` order (`rareCandy, mint, shinyCharm`) filtered count > 0.
- Rare Candy availability: active Pokémon + evolution line loaded + count > 0 + not mid
  Ditto-reveal (`:1166-1193`).
- Stage costs = difficulty-scaled thresholds for stages `stageIndex ..< totalForms`.
  `remaining = sum(costs) − usedAtStage`; `needed = ceil(remaining / 100M)`;
  `maxRareCandyUseCount = min(candyCount, needed)`.
- `planRareCandyUse(count)` (`:1204-1222`): `remaining = usedAtStage + count×100M`; walk costs;
  if the last stage is cleared → `graduates = true, discardedXP = remaining leftover`; else
  `evolves = (≥1 stage cleared)`, `carryoverXP = evolves ? remaining : 0`.
- `useRareCandy(count)` (`:1229-1249`): consumes the plan count (Ditto disguise: capped to the
  candies needed to reach the reveal); subtracts inventory; sets candy feedback amount; calls the
  normal growth path `applyUsage(xp)` (evolution, graduation, notifications, save). Candy XP does
  **not** add to `usedSinceInstall`, today, or the wallet. Result: `evolved / graduated / progressed`.
- Mint (`:1251-1271`): requires active + count > 0 (no line requirement). Pool = 25 natures minus the
  current one (all 25 if current is nil). `rng.next() % pool.count`. Decrement, feedback, save.
- Shiny Charm: owning ≥ 1 = effect active; no use button.

### 3.6 Candy grants from 100% limits

Window model (`CompanionModel.swift:293-320`): `CandyWindow { key, name, kind: session|weekly,
utilization (0…100+), epoch: String? }`.

Eligible windows (`UsageStore.swift:776-847`):

| Provider | key | name (en) | kind | epoch |
|---|---|---|---|---|
| Claude (default account) | `claude.fiveHour` | "Claude 5-hour session" | session | ISO8601 of `resets_at` (or raw string) |
| Claude (default account) | `claude.sevenDay` | "Claude weekly" | weekly | same |
| Claude extra account | `claude.<accountKey>.fiveHour/.sevenDay` | + " (title)" when >1 account | — | only after that key was once seen < 100% (`claudeAccountCandyArmed` UserDefaults) |
| Codex bucket | `codex.<limitId ?? limitName ?? "codex">.primary` / `.secondary` | "`Bucket` `window`" (e.g. "Codex 5-hour session", "Codex Weekly") | `minutes > 1440` → weekly, else session (nil → session) | **none** (since #434) |
| Antigravity | `antigravity.(gemini|3p).5h` / `.weekly` | group title + window | session/weekly | reset date ISO8601 |

Excluded: Opus/Sonnet weekly, scoped windows, Codex spend caps.

`evaluateCandyGrants` (`CompanionStore.swift:1454-1490`), per window:

```text
if epoch != nil:
    if windowEpoch[key] exists:
        if isNewEpoch(prev, epoch, kind): grantTier[key] = nil; windowEpoch[key] = epoch
    else: windowEpoch[key] = epoch
if utilization < 100: grantTier[key] = nil; (epoch? windowEpoch[key] = epoch); continue
if grantTier[key] ?? 0 >= 1: continue
grantTier[key] = 1; (epoch? windowEpoch[key] = epoch)
grant count = weekly ? 5 : 1
```

`isNewEpoch(previous, current, kind)` (`:1436-1444`, #375 / v2.5.6):
- equal strings → false;
- both parse as dates → new only if `current − previous ≥ 86400 s` (weekly) or `≥ 1800 s` (session);
  a backward or small jitter (Codex ±1 s, Claude microseconds) is **not** a new epoch;
- either fails to parse → true (string inequality).
- `parseEpochDate`: numeric > 1e12 → milliseconds; > 1e8 → seconds; else ISO8601.

`grantCandies(windows, limitsReady)` (`:1493-1530`):
- `limitsReady == false` → do nothing (no seed, no grant). `limitsReady` = any provider limit loaded.
- First run (`candyFeatureSeeded == false`): for every window already ≥ 100% set tier 1 and store
  epoch; set seeded; save; return (no grants).
- Otherwise evaluate; add `count` Rare Candy per grant; notification per grant
  (`showsOnPet: false`); persist when grants happened **or** tier/epoch maps changed (rearm must
  persist, otherwise a restart sees a stale tier 1 and misses the next grant).
- Notification text (en): title `🍬 You got {count}× Rare Candy!`, body `You maxed out your {window}
  token limit. A treat for the effort — use it to evolve your Pokémon!` (ko: `🍬 이상한 사탕
  {count}개를 받았어요!` / `{window} 토큰 한도를 다 채웠어요. 열심히 쓴 만큼 사탕을 드려요 — 포켓몬에게
  써서 진화시켜 보세요!`) (`Localization.swift:1961-1980`).
- Gated by `companionNotifications` (default true) for the system notification only; the grant
  itself is unconditional.
- Save import merges `candyGrantTier` per key with `max`, epochs: both granted → later epoch,
  else the granted side's epoch; `candyFeatureSeeded = a || b` (`SaveTransfer.swift:208-260`).

### 3.7 Regional Pokédex (`PokedexRegion.swift`)

| region (rawValue) | dex range | total | en | ko | ja |
|---|---|---|---|---|---|
| `kanto` | 1–151 | 151 | Kanto | 관동 | カントー |
| `johto` | 152–251 | 100 | Johto | 성도 | ジョウト |
| `hoenn` | 252–386 | 135 | Hoenn | 호연 | ホウエン |
| `sinnoh` | 387–493 | 107 | Sinnoh | 신오 | シンオウ |
| `unova` | 494–649 | 156 | Unova | 하나 | イッシュ |

National range 1–649 (total 649). Owned input = `dexSpecies` (species ids, shiny = any owned
individual shiny; includes active reached stages and released records). Counts are per species id
(forms do not add). `isComplete = owned >= total`, `isShinyComplete = shinyOwned >= total`.

Badges (`DexBadge`) and persisted `notificationKey`s (must never change):
`region.<r>`, `regionShiny.<r>`, `unown` (all 28 Unown forms collected:
`state.collectedForms(201).count >= 28`), `national`, `nationalShiny`. Order of `earnedBadges`:
for each region in order (region, regionShiny), then unown, national, nationalShiny.

Notifications fire from `save()` → `checkDexCompletions()` for badges not in
`notifiedDexCompletions`; inserted keys are saved in the same write. `showsOnPet: true`.
First run (`dexCompletionsSeeded == false`): mark all currently earned keys as notified without
notifying; if none, don't write the file just for the seed (`CompanionStore.swift:518-541`).
Texts (en/ko): region `🏅 {R} Dex complete!` / `You've caught every Pokémon in {R}!`
(`🏅 {R} 도감 완성!` / `{R} 지방의 모든 포켓몬을 모았어요!`); shiny region `✨ Shiny {R} Dex complete!` /
`You've caught a shiny of every Pokémon in {R}!`; Unown `🔤 Unown collection complete!` /
`You've collected all 28 Unown forms!`; national `🌍 National Dex complete!` /
`You've caught every Pokémon from Gen 1–5!`; national shiny `✨ Shiny National Dex complete!` /
`You've caught a shiny of every Pokémon!` (`Localization.swift:1494-1542`).

### 3.8 Pokédex species model (`CompanionStore.swift:452-597`)

- Species set = every `dex[].chainOrder` id ∪ active `pathIDs[0...stageIndex]` (never
  `plannedPathIDs`). Main grid groups forms (one cell per species); detail groups by form.
- Per species: `rarity` = first seen line rarity; `names` = last non-nil names; `isShiny` if any
  shiny individual; `hasNormal` if any non-shiny; `isRaising` = id == active current species (and
  same form in the form view).
- Unnamed species fall back to `#id`. Sorted by (id, form sortOrder).
- Catch log entries = `dex` + synthesized active entry (id `active-<baseID>-<currentID>`,
  `caughtAt = nil`, chain = full `pathIDs`).

### 3.9 Search (`DexSearchMatcher`, `CompanionStore.swift:621-643`)

- Trim whitespace; empty → no filter.
- `number = Int(query without leading "#")`: exact id match if any candidate id equals it.
- Also matches if any `"#\(id)"` string **contains** the raw query (so `25` finds #25, #125, #250;
  `#25` finds #25, #250 but not #125).
- Names: any stored name in any language contains the query, case- and diacritic-insensitive
  (`flabebe` → Flabébé).
- Species candidates: `[species.id]`, names: display name + all languages. Catch-log candidates:
  `[baseID, finalID] + chainOrder`, names: every language of every chain species.

### 3.10 Filters and sorting

Pokédex filter order: rarity → region (`dexRange.contains(id)`) → shinyOnly → search → sort.
Catch log: rarity → shinyOnly → search → sort (no region filter).

`DexSortOption` (`numberAsc` default, `numberDesc`, `nameAsc`, `nameDesc`, `rarityDesc`):
name uses localized compare, tie → id asc; rarity desc by sortRank, tie → id asc.

`CatchLogSortOption` (`recentFirst` default, `oldestFirst`, `numberAsc`, `numberDesc`, `nameAsc`,
`nameDesc`, `rarityDesc`) (`:717-800`):
- recentFirst: active first, then `caughtAt` desc (nil = distant past), tie id asc.
- oldestFirst: `caughtAt` asc, tie id asc, active **last**.
- number: `finalID` asc/desc, tie `caughtAt` desc, then id asc.
- name: `dexEntryName` (final species name, else last named chain species, else `#finalID`),
  localized compare, tie `caughtAt` desc, id asc.
- rarityDesc: sortRank desc, tie `caughtAt` desc, id asc.

Sort labels (en / ko): Number (Low to High) / 도감 번호 (낮은 순); Number (High to Low) / 도감 번호 (높은 순);
Name (A–Z) / 이름 (가나다·A-Z); Name (Z–A) / 이름 (역순); Rarity (High to Low) / 희귀도 순;
Most Recent / 최근 잡은 순; Oldest First / 오래된 순.

### 3.11 Paging (`CompanionView.swift:1381-1430, 1473-1530`)

- `pageSize = 16`, `pageCount = max(1, ceil(n/16))`, `current = min(page ?? pageOf(returnAnchor), pageCount−1)`.
- Any change of search, shiny, sort, rarity, region → page 0, selection cleared.
- Wheel: one notch (non-precise) = one page; delta < 0 → next. Trackpad (precise): accumulate
  until |Σ| ≥ 24 pt, then one page per gesture; momentum ignored; reset on began/ended/cancelled.
- `page(containing:)` = `index / 16` of the anchor's `collectionID` in the filtered list, 0 if absent.

### 3.12 Detail page rules (`CompanionView.swift:1722-2077`, `CompanionStore.swift:2142-2175`)

- Opened via `nav.dexDetailCollectionID = species.collectionID` (`"<id>"` or `"<id>-<form>"`).
  Cleared when leaving the tab, switching segment, or opening Settings/Recap
  (`PopoverView.swift:35-47`). Home sprites can open it (`openDexEntry`).
- Individuals = `dexEntriesSorted` filtered `finalID == speciesID && profile != nil && same form`
  (earlier evolution stages are reference pages without individuals).
- Appearance picker shown only when the (form-)species has both normal and shiny. Displayed shiny:
  explicit selection if available, else first individual's shininess, else `species.isShiny`.
- Individual picker shown only when > 1 individuals of the displayed appearance (#429). Label:
  `#{index+1} · {✨ Shiny|Normal} · Lv. {level ?? 5} · {date numeric | Raising | —}`.
- Form picker columns: `formCount > 8 ? (formCount > 20 ? 7 : 6) : min(formCount, 4)`; uncollected
  forms visible at 25% opacity and disabled; header `Forms {owned}/{total}`.
- Details fetched from `https://pokeapi.co/api/v2/pokemon/{id}`, cached in
  `pokemon-details-v1/` (`PokeAPIClient.swift:35-55`); version group `black-2-white-2`.
- Stat bar scale: base stats → 300; actual stats → `max(300, maxValue)` rounded up to 100.
- Back: from grid, set `dexReturnAnchor = id` so the rebuilt grid opens on that page (used once);
  from catch log, `logReturnAnchor = entry.id`, scroll to it centered, then again after 50 ms.

---

## 4. Data / persistence

macOS file: `~/Library/Application Support/PokeTokenBar/companion-state.json` (override
`PTB_STATE_DIR`). Atomic write. Lenient decode per field. Relevant fields of `CompanionState`
(`CompanionModel.swift:648-750`):

| JSON key | type | default | meaning |
|---|---|---|---|
| `usedSinceInstall` | Int | 0 | lifetime growth tokens (wallet income) |
| `spentTokens` | Int | 0 | shop spend ledger |
| `eggUsage` | Int | 0 | egg incubation |
| `eggTier` | `"uncommon"`/`"rare"`/null | null | bought guarantee; unknown/legendary → null |
| `pendingHatchID`, `pendingForm` | Int?, String? | null | pre-rolled hatch |
| `inventory` | `{ "rareCandy": n, "mint": n, "shinyCharm": 1 }` | {} | keys = `ItemKind.rawValue` |
| `candyGrantTier` | `{ windowKey: 1 }` | {} | granted windows |
| `candyWindowEpoch` | `{ windowKey: epochString }` | {} | last seen epoch |
| `candyFeatureSeeded` | Bool | false | first-run seed done |
| `notifiedDexCompletions` | [String] (set) | [] | badge notification keys |
| `dexCompletionsSeeded` | Bool | false | badge seed done |
| `dex` | [DexEntry] | [] | lossy per entry |
| `collectedFinals` | ["base:final"] | [] | only graduation inserts |
| `representativeSpeciesID`, `representativeForm` | Int?, String? | null | pinned species |

`DexEntry` JSON: `id` (String, UUID or profile instanceID), `baseID`, `finalID`, `chainOrder`,
`rarity`, `caughtAt` (Date, Swift default encoding = seconds since 2001-01-01), `isShiny`, `nature`,
`form`, `profile`, `prayers` (`{count}`), `names` (`{speciesID: {lang: name}}`), `namesVersion`
(current 1), `releasedAt` (Date?; non-nil = released record).

Sanitizing on load/import (`SaveTransfer.swift:142-190`): token fields clamped to
`0…1_000_000_000_000_000`; `eggTier` and `pendingHatchID` dropped if an active Pokémon exists;
`eggTier` with nil ceiling (legendary) dropped.

UserDefaults: `shopDifficulty`, `growthDifficulty`, `companionNotifications` (default true),
`claudeAccountCandyArmed` ([String]).

---

## 5. UI spec

Popover: width 360, padding 14 → content width 332; scroll content keeps a 12 pt right lane
(`reservesScrollerLane`) → 320 usable. Shop, Bag and Collection use a fixed content height of
**520** so the popover never resizes between tabs.

Colors (SwiftUI system → hex light / dark):
- green `#34C759 / #30D158`, blue `#007AFF / #0A84FF`, orange `#FF9500 / #FF9F0A`,
  gray `#8E8E93 / #8E8E93`, yellow `#FFCC00 / #FFD60A`, accent (default blue) `#007AFF / #0A84FF`.
- `.secondary` text `rgba(60,60,67,.60) / rgba(235,235,245,.60)`; `.tertiary`
  `rgba(60,60,67,.30) / rgba(235,235,245,.30)`; card fill `Color.secondary.opacity(0.06)` ≈
  `rgba(60,60,67,.036) / rgba(235,235,245,.036)` (screenshot dark card ≈ `#2E2E2E` on `#262626`).
- controlBackground `#FFFFFF / #1E1E1E`; windowBackground `#ECECEC / #323232`.
- Rarity colors (`rarityColor`, `CompanionView.swift:3-10`): common gray, uncommon green, rare blue,
  legendary orange.
Fonts (macOS points): caption/caption2 10, callout 12, body 13, title3 15; "callout.semibold" = 12/600.
Rarity chip display order: Legendary, Rare, Uncommon, Common.

### 5.1 Shop (`ShopView.swift`)

```
ScrollView (h=520) > VStack(spacing 10, lane 12)
  WalletHeader  pad 10, radius 10, card fill
    "Spendable tokens"   caption, secondary
    compact(available)   24pt bold, tabular
    "Spend the tokens you've used on items."  caption2, tertiary
  ForEach shopEntries → ShopItemCard | EggCard
```

ShopItemCard (pad 10, radius 10, card fill, VStack spacing 8):
- Row (top-aligned, spacing 10): ItemIcon 30×30; column (spacing 2):
  - name callout semibold; if owned>0 and not passive: `Owned ×n` caption2 bold secondary;
    spacer; Stepper only if `!passive && maxBuyCount > 1` showing `×{q}` callout semibold.
  - description caption secondary, wraps.
- Controls row:
  - passive & owned: green `checkmark.seal.fill` + "Owned" caption2 semibold green.
  - confirming: `Buy {name[ ×q]}?` caption2 secondary (1 line) · spacer · prominent small
    `Buy[ ×q]` · borderless `Cancel`.
  - idle: `Price {compact(price×q)}` caption2 tertiary · spacer · bordered small `Buy` if
    `canBuy` else "Not enough tokens" caption2 tertiary.
- Quantity clamps to `max(1, maxBuyCount)` when the wallet shrinks; reset to 1 after success.

EggCard: same card; icon = egg sprite 26 in a 30 frame; name (`Pokémon Egg` / `Uncommon Egg` /
`Rare Egg`) + solid rarity Badge with uppercased label (8pt bold white on rarity color, h-pad 5,
v-pad 1, capsule) for tiered eggs; description; if active exists: release note caption2 tertiary.
Controls:
- idle: price · Buy disabled (egg stage) / Buy (canBuyEgg) / "Not enough tokens"; in egg stage add
  hint "Available once your current egg hatches." caption2 tertiary.
- confirm: `Send off {name} for the {egg}?` (2 lines) · prominent `Buy` (→ precious step if
  high-value, else commit) · `Cancel`.
- preciousConfirm: orange semibold caption2 warning: legendary → "⚠️ This is a Legendary Pokémon!
  Really send it off?", else "⚠️ This one is shiny! Really send it off?"; orange prominent button
  "Send shiny off" (shiny and not legendary) or "Send off"; Cancel.
- commit → `buyEgg` → on success tab = home.

Strings (en/ko): wallet "Spendable tokens"/"쓸 수 있는 토큰"; hint; "Buy"/"구매"; "Cancel"/"취소";
"Price"/"가격"; "Owned ×n"/"보유 ×n"; "Owned"/"보유 중"; "Not enough tokens"/"토큰이 부족해요";
eggDescription untiered "Send off your current Pokémon and start fresh with a new egg.", tiered
"Send off your current Pokémon for an egg guaranteed to hatch {R} or better."; release note "A
released Pokémon stays in your Pokédex and can hatch again at the same odds — only the growth
progress is lost." Item descriptions: candy "Raises your Pokémon's EXP by 100M.", mint "Randomly
changes your Pokémon's nature.", charm "While owned, raises the chance of hatching a shiny."
Item names en/ko/ja: Rare Candy/이상한 사탕/ふしぎなアメ, Mint/민트/ミント, Shiny Charm/이로치 부적/ひかるおまもり.

### 5.2 Bag (`BagView.swift`)

- Empty: VStack spacing 10, vertical pad 28: animated Snorlax (#143) 96pt; "Your bag is empty!"
  callout semibold; bordered small "Shop" button → shop tab. (Not fixed-height.)
- List: ScrollView h=520, VStack spacing 8 of ItemCards (pad 10, radius 10, card fill):
  - name + `×count` caption bold secondary (not for passive); stepper for candy when usable.
  - candy preview lines (caption2): graduates → "Expected to graduate." secondary; discarded > 0 →
    "On graduation, {xp} leftover XP will be discarded." **orange**; else evolves →
    "Expected after evolution: {xp} XP carried over." secondary.
  - controls: passive → green seal + "Shiny rate ↑ · active"; usable idle → hint
    (`+{compact(q×100M)} XP` / "Random nature") caption2 tertiary · bordered small "Use";
    confirming → "Use on {name}?" · prominent `Use[ ×q]` · Cancel; not usable →
    egg stage "Usable after hatching" else "No Pokémon to use it on" caption2 tertiary.
  - Use → perform → tab = home.

### 5.3 Collection (`CompanionView.swift:1049-1330`)

Empty (no entries at all): animated Pikachu 96, "No Pokémon caught yet!" callout semibold, "Spend
tokens to hatch your first Pokémon." caption secondary centered, vertical pad 28.

Otherwise VStack spacing 6, height 520:
1. Segmented picker (labels hidden): "Pokédex" | "Catch log".
2. Toolbar HStack spacing 6:
   - Search box: magnifier 13pt secondary, plain text field 13pt, placeholder "Search name or #…",
     clear ✕ (xmark.circle.fill) when non-empty; pad h6 v4, controlBackground, radius 6, border
     0.5 `secondary@0.2`.
   - Shiny toggle: `sparkle` (off) / `sparkles` bold (on) 13pt in a 16×16 frame, pad h6 v4, radius 6;
     on: yellow glyph, fill `yellow@0.18`, border `yellow@0.6`; off: secondary glyph,
     controlBackground, border `secondary@0.2`; toggle animated 0.15 s; tooltip "Shiny only".
   - Sort menu: `arrow.up.arrow.down` 13pt; active (non-default) accent glyph, fill `accent@0.12`,
     border `accent@0.5`; menu items with ✓ on the current option; tooltip "Sort".
3. Content (grid or log).

Search text, shiny toggle and rarity selection are **shared** by both segments; region and dex sort
belong to the grid, log sort to the log. "Reset filters" clears all six.

#### Pokédex grid (`DexGridView`)

Header VStack spacing 5:
- HStack: "Pokédex" callout semibold, "{n} species" caption secondary, spacer, medals (only
  unown/national/nationalShiny: 🔤 🌍 ✨) each 12pt emoji, pad h5 v2, fill `yellow@0.14`, border
  `yellow@0.5` 0.5, capsule, tooltip badge label.
- Region chips (horizontal scroll, spacing 4) `RegionTally`: prefix ✨ (shiny complete) or 🏅
  (complete) 10pt; label 11pt (semibold when selected, else medium); `owned/total` 11pt bold —
  green when complete else secondary; ✓ 9pt bold when selected; pad h6 v2; capsule; fill
  `accent@(selected .22 | complete .14 | .08)` where accent = green if complete else secondary;
  border selected 1.5 accent, else 0.5 `accent@0.35`; opacity 0.5 and disabled when owned 0;
  tooltip "{Region} {owned}/{total}". Tap toggles region filter.
- Rarity chips `RarityTally`: 6×6 colored dot, label 11pt, count 11pt bold, ✓ when selected, pad
  h6 v2, fill `color@(selected .22 : .12)`, border selected 1.5 color else 0.5 `color@0.35`,
  opacity 0.4 + disabled when count 0, tooltip "Tap to show only this rarity · tap again to clear".
  Counts = species of that rarity in the **unfiltered** dex.

Grid: 4×4, spacing 4, rows stretch to fill the remaining height; empty slots are blank.

Cell (`DexSpeciesCell`, `:2080-2208`):
- Button; VStack spacing 1: static sprite 56×56 (shiny variant only if species has no forms and
  isShiny), RAISING badge overlaid at the sprite bottom (tinted accent 10pt bold uppercase on
  regular material capsule); name 12pt, 2 lines reserved, centered; for form species
  `"{name} {owned}/{total}"`.
- Overlays: top-left number capsule `#id` 10pt semibold in rarity color (+ accent ★ 8pt when
  representative) on material; top-right ✨ 10pt on material if shiny owned.
- Padding 2; background: representative `accent@0.16`, selected `secondary@0.16`, else
  `secondary@0.06`; radius 8; selected stroke 1.5 accent.
- Hover: scale 1.04, card-shaped windowBackground fill with shadow black@0.25 r4 y2, 0.12 s ease-out.
- Tooltip / a11y: `#id name · Rarity[ · Forms a/b][ · Shiny][ · Raising][ · Representative]`.
- Click: select + open detail. Context menu: forms species → "Choose form" (opens detail); else
  "Set as representative" (star) / "Follow current" (arrow.triangle.2.circlepath) toggle.

Footer (height 18, 13pt semibold): selected `#id name · Rarity` 11pt secondary + icon-only mini
representative button (no-form species); spacer; when >1 page: ‹ `{c} / {n}` (12pt semibold
secondary tabular) › with 28×28 hit areas, outer padding −10 so glyphs sit at the edge; disabled at
the ends.

No results: magnifier 24 tertiary, "No Pokémon found" callout medium secondary, bordered small
"Reset filters", vertically centered.

#### Catch log

Header: "Catch log" callout semibold + "{n} total" caption secondary (n = all individuals incl.
active); rarity chips with **individual** counts. List: ScrollView, LazyVStack spacing 8. Filter,
search, shiny or sort change scrolls back to top (0.2 s).

Row card (pad 8, radius 10, card fill, VStack spacing 3):
- HStack: solid rarity badge (10pt uppercase); RAISING (accent tinted) if active, else RELEASED
  (secondary tinted) if released; ✨ 10pt if shiny; spacer; nature 11pt secondary.
- Evolution line: chain sprites thumb 68 with arrows (arrow width = 25% of thumb, spacing 2),
  species names 11pt under each (2 lines), horizontally scrollable when wider than 296
  (320 − 2×8); sprites link to the dex detail when the species is in the dex.
- `caughtAt` relative time (no "ago") 11pt secondary, e.g. "2 yrs, 10 mths"; hidden for active.
- Names: stored names, else async PokéAPI line fetch that back-fills all entries of the same base.

#### Detail page

VStack spacing 8: header row (borderless "‹ Back", spacer, `#id` callout secondary); ScrollView
VStack spacing 12:
- Form picker (if species has forms): "Forms a/b" callout semibold; grid with spacing 3/4; tile =
  36×36 sprite (shiny if owned shiny, ✨ 11pt top-right), label 11pt semibold (1 line when >4 cols,
  else 2 lines reserved, min scale 0.6); tile pad v3; fill accent@0.16 if representative else
  secondary@0.06; radius 6; selected stroke 1.5 accent; uncollected opacity 0.25 + disabled.
- Appearance segmented "Normal | ✨ Shiny" (only when both owned).
- Identity: 104×104 animated sprite; name title3 bold (`Rotom [Heat Rotom]`, Unown `Unown [A]`),
  rarity callout semibold secondary right-aligned; type badges 11pt (pad h7 v2) with game colors;
  "Data uses the default form" caption2 for non-default forms; "✨ Shiny"; "Raising" in accent for
  the active individual; representative icon button.
- Individual menu picker "Individual" (only > 1).
- No individuals → "Collected appearance · species reference" callout secondary.
- With details: individual card (Level, Gender, Nature, prayers lines, Ability with " · Hidden
  Ability", Actual stats rows [label w74, bar, value w30, `IV n` w38, 12pt], Known moves list
  `name … Lv. n`) or Base stats card; Species data card (Height `%.1f m`, Weight `%.1f kg`, Base
  stat total, Possible abilities with " (Hidden)"); full move list card (`Moves (n)`, rows with
  learn methods joined by " · ", dividers). Cards: pad 9, radius 9, fill `secondary@0.07`.
- Loading: spinner + "Loading Pokémon details…"; error: "Pokémon details could not be loaded." +
  Retry.

---

## 6. Windows adaptation

| macOS mechanism | Windows / Tauri equivalent |
|---|---|
| `UNUserNotificationCenter` (candy grant, dex badges) | `tauri-plugin-notification` (already registered, `lib.rs:139` `notify`). Gate on `settings.companion_notifications`. Also emit `companion-event` for the pet pop (badges yes, candy no). |
| `UserDefaults shopDifficulty` | `shop_difficulty: f64` in `settings.json` (`settings.rs` `Settings`, sanitize with `clamp_difficulty`). Keep it out of the companion save, same rationale as macOS. |
| `UserDefaults claudeAccountCandyArmed` | Not needed until multi-account Claude exists on Windows; if added, store in settings/aux JSON. |
| Application Support `companion-state.json` | Existing `companion::Store` path in app data dir; add fields to `State` (struct already `#[serde(default)]`). |
| NSPopover tabs (home/shop/bag/collection) | Main Tauri window (360×680) `nav.tabs`: add `shop`, `bag`, rename `dex` → collection. Keep a fixed 520 px content height or let it scroll; window is fixed size anyway. |
| `.sheet` avoidance / inline confirm | Same inline morph pattern in React (no modal). Replace current `SpeciesDetail` modal with an in-tab page. |
| SwiftUI `Stepper` | Custom `− ×n +` control or a vertical up/down pair; clamp 1…max. |
| `Menu` with checkmarks | Custom popover menu (absolute-positioned list) or `<select>`; must show ✓ on the active option and accent-tint the trigger when non-default. |
| `.help()` tooltips | `title` attribute. |
| `.contextMenu` | `onContextMenu` + custom menu; or Tauri `Menu.popup()` from `@tauri-apps/api/menu`. |
| `NSEvent` scroll monitor paging | React `onWheel` on the grid: `deltaMode === 1` (lines) or `|deltaY| ≥ 100` per event → one page per event; pixel deltas (touchpad) accumulate to 24 px with a ~150 ms idle reset to emulate gesture boundaries; `preventDefault` via a non-passive listener. |
| `NSCursor.pointingHand` | CSS `cursor: pointer` on link sprites/cells. |
| `.regularMaterial` capsules | `background: color-mix(in srgb, var(--card) 80%, transparent); backdrop-filter: blur(6px)`. |
| `Text(date, style: .relative)` | Custom formatter: two largest units, abbreviated, no "ago" (`Intl.NumberFormat` unit style `short` per unit, joined by ", "). |
| `localizedCompare` / diacritic-insensitive search | `Intl.Collator(lang, { sensitivity: "base" })` for sort; search via `s.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase()`. |
| System colors (light/dark) | CSS variables with the hex pairs in §5 under `prefers-color-scheme`. Windows accent today is `#2da44e`; rarity colors must use the Apple values regardless. |
| SpriteView static vs animated | Grid: static PNG `…/sprites/pokemon/{id}.png` / `shiny/{id}.png` (not 16 animated GIFs). Detail/identity: Gen V animated GIF (existing `sprite()`). Items: `…/sprites/items/{name}.png` with emoji fallback (`onError`). |
| Limit windows for candy | Build `CandyWindow`s in Rust from `limits::ProviderLimits`: Claude `5h` → `claude.fiveHour` session, `7d` → `claude.sevenDay` weekly (skip `7d-opus`, `7d-sonnet`); epoch = `resets_at`. Codex: key `codex.codex.primary` / `.secondary` (slot by order), kind from window minutes (> 1440 → weekly), **no epoch**. `limitsReady` = any provider with non-empty windows. Run `grant_candies` at the end of `refresh_limits`. |

---

## 7. Gaps vs current Windows code

Backend (`src-tauri/src/companion.rs`, `lib.rs`):
1. No wallet: `State` lacks `spent_tokens`; no `available_tokens`.
2. No `inventory`, `ItemKind`, prices, `buy`, `maxBuyCount`, shop entries/sorting, shop difficulty.
3. No eggs for sale: no `buy_egg`, no `egg_tier`, no guaranteed hatch filter in `choose_base`
   (`companion.rs:382`) or post-fetch rarity gate in `hatch_if_ready` (`lib.rs:187-206`).
4. No release path: `DexEntry` lacks `id`, `released_at`, `form`, `profile`, `prayers`,
   `names_version`; no released record creation.
5. No Rare Candy use / plan / max count; no mint; no shiny charm (shiny denominator hard-coded 64,
   `companion.rs:11, 319`).
6. No candy grants: no `candy_grant_tier`, `candy_window_epoch`, `candy_feature_seeded`,
   `is_new_epoch`, `parse_epoch_date`, notifications.
7. No regional progress / badges / `notified_dex_completions` / `dex_completions_seeded` / badge
   notifications.
8. No Tauri commands for buy/use/egg; `view()` (`lib.rs:126`) does not expose wallet, inventory,
   prices, guarantee, completions or badges.
9. Codex limit parser zeroes `used_percent` once `resets_at` passes (`limits.rs:149-151`); with the
   no-epoch rule this acts as a sub-100% observation and rearms Codex candy at reset — the macOS
   v2.5.7 fix (#434) exists precisely to *not* rearm while Codex stays at 100% on credits. Candy
   evaluation must ignore synthetic 0% from expired snapshots (or use the raw value).

Frontend (`src/Dex.tsx`, `src/App.tsx`, `src/shared.ts`):
10. No Shop and Bag tabs/components at all (`App.tsx:175-188` has home/dex/settings).
11. Search: numeric query uses `startsWith` (`Dex.tsx:12`) — macOS uses exact id or substring of
    `#id` (`25` must match #125); `#` handling differs; no diacritic folding; catch log only checks
    `final_id` for numbers (macOS: base, final, chain).
12. Rarity filter is a `<select>` (`Dex.tsx:34`) instead of chips with counts, disabled-at-zero,
    toggle-off.
13. Shiny filter is a checkbox and missing in the catch log; macOS has one shared toggle button.
14. Filters are per-segment state; macOS shares search/shiny/rarity across segments.
15. Dex sorts: missing `nameDesc`, `rarityDesc`; catch log sorts: missing number asc/desc, name
    asc/desc, rarity; oldest-first placement of the active entry.
16. No region chips, progress, medals.
17. Grid cell: animated GIF for all 16 cells and never shiny (`Dex.tsx:148`); number zero-padded
    `#013` (macOS `#13`, rarity-colored capsule); no RAISING badge on sprite, no ✨/★ capsules,
    no hover scale/shadow, no tooltip, no context menu, no forms count.
18. Pager: `‹ ›` text buttons with small hit area, no wheel paging, no footer selection line /
    representative button.
19. Detail is a modal (`Dex.tsx:56-99`) with name, rarity, count, shiny toggle, representative;
    missing: Back page, `#id` header, forms picker, appearance segmented, identity header 104 px,
    type badges, individual picker, profile/stats/moves/species data, loading/error/retry,
    return-to-page/row anchors.
20. Catch log omits the active (raising) individual; no RAISING/RELEASED badges; no arrows or names
    under chain sprites; absolute date instead of relative; row key is `caught_at+index` (no id).
21. Empty states: plain text instead of animated Pikachu/Snorlax; no "No Pokémon found" + Reset.
22. Forms (Unown etc.) and profiles are not modelled at all on Windows — the detail page parity
    depends on porting `PokemonForm.swift` and `PokemonProfile.swift` (separate area).
23. i18n has only ko/en/ja (`i18n.ts:1`); all strings in §5 need adding for these three.

---

## 8. Open questions

1. Codex bucket key on Windows: rollouts may not expose `limit_id`/`limit_name`; propose fixed
   `codex.codex.primary|secondary` to stay save-compatible with macOS. Confirm whether macOS save
   import is a goal for Windows.
2. Should Windows keep growth difficulty in the save (current `State.growth_difficulty`) while
   macOS stores it in UserDefaults? Shop difficulty is proposed for `settings.json` either way.
3. Profiles/forms/Ditto/prayers are absent on Windows; the detail page, released-record
   `profile`, and individual picker need those models first. Ship the detail page in two steps
   (species data now, individual section later)?
4. macOS screenshot `screenshot-pokemon-forms.png` shows the individual picker with one
   individual; code (#429) hides it unless > 1. Follow the code.
5. `screenshot-collection-pokedex.png` predates region chips (v2.5.7 adds them, see
   `screenshot-regional-dex.png`). Follow the code.
6. Dates in `DexEntry.caughtAt`: Windows stores RFC 3339 strings; macOS uses Swift `Date`
   reference-date seconds. Only matters if cross-platform save transfer is planned.
