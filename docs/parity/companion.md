# Companion: parity spec (PokeTokenBar v2.5.7 → Windows)

Source of truth: PokeTokenBar v2.5.7, `Sources/PokeTokenBar`. In this spec, `Model:` means `Core/CompanionModel.swift`, `Store:` means `Core/CompanionStore.swift`, `View:` means `UI/CompanionView.swift`, `Form:` means `Core/PokemonForm.swift`, `Profile:` means `Core/PokemonProfile.swift`, `API:` means `Core/PokeAPIClient.swift`, `Names:` means `Core/PokemonNameLocalization.swift`, and `Sprite:` means `UI/SpriteLoader.swift`. Line numbers come from that tree.

Target: `src-tauri/src/companion.rs`, `src-tauri/src/pokeapi.rs`, `src-tauri/src/lib.rs` (`refresh`, `hatch_if_ready`, `view`) and `src/App.tsx` (`CompanionCard`), as of v0.2.3 (`c8a0288`).

Out of scope here: the shop/bag purchase UI, the candy grant UI, the Pokédex grid, the catch log and the detail page. This spec covers only the rules those screens rely on: release records, representative species and profiles.

---

## 1. Overview

Token usage from local AI coding tools grows one companion. The lifecycle runs: **egg → hatch → evolve along a pre-planned path → graduate into the Pokédex → a new egg arrives**. Five things are fixed at hatch and never change afterwards:

- species (a capture-rate-weighted roll over Gen 1–5 line starts)
- shiny
- nature
- alternate form
- individual profile: IVs, plus the gender, ability and moves that are filled in later

One growth meter (`usedSinceInstall`) also funds a wallet, defined as `usedSinceInstall − spentTokens`. A rare 1/128 Ditto disguise reveals itself at the first evolution threshold. All Pokémon data and sprites come from PokéAPI at runtime and are cached on disk. Nothing is bundled.

## 2. Features

1. **Per-provider usage ledger.** Only usage after install is counted, and each provider is tracked separately. There is no retroactive credit.
2. **Egg incubation.** The egg needs 5M tokens × the growth difficulty. Any overflow carries into the hatchling.
3. **Egg prefetch.** While incubating, the app pre-rolls the species and form, persists them, and warms the line, the sprites and the form names.
4. **Hatch roll.** The species is capture-rate weighted, and lines already collected count half. Guaranteed-tier eggs filter the candidates first. A REST fallback covers GraphQL outages.
5. **Individual rolls.** Shiny is 1/64, or 1/48 with the Shiny Charm. Nature is one of 25. The Ditto disguise is 1/128 for common lines with ≥2 forms. The form roll favours forms not yet collected. The profile seed is rolled here too.
6. **Evolution plan.** The full branch route is chosen at hatch and prefers branches whose final form is uncollected. Saved plans that turn out invalid are repaired.
7. **Staged growth.** For a line of k forms, stage i costs `T·i/(k(k+1)/2)`, where T depends on rarity. All stages of a line sum to T.
8. **Repeat-growth boost.** If this line's final form was graduated before, every stage threshold is halved (×2 growth).
9. **Growth difficulty slider.** Range 0.1–2.0 on a log scale. Changing it rescales banked progress and never evolves or hatches anything.
10. **Graduation.** When the final stage threshold is reached, the line is saved to the dex with names, profile and form. `collectedFinals` gets `"base:final"` and a new egg starts.
11. **Release.** Buying a new egg releases the active Pokémon. The release record stays in the dex, and `collectedFinals` is left untouched.
12. **Ditto disguise and reveal.** At the first evolution threshold the disguised Pokémon turns out to be Ditto (rare, single form). Overflow carries over and the profile is rebased.
13. **Unown's 28 letters and the other Gen 1–5 alternate forms.** The form is chosen per individual and resolved per species of the line.
14. **Individual profile.** IVs, gender, ability (1/128 hidden), the last 4 level-up moves from BW2, a level from 5 to 100 tied to growth, actual stats with nature modifiers, and Shedinja's HP fixed at 1.
15. **Representative species.** The user can pin a Pokédex species and form for the menu bar and floating pet. The pin resets automatically when the species stops being owned.
16. **Display state.** One of egg, idle, working, focus, tired, sleep, sulk or levelUp, driven by burn rate, limits and idle time.
17. **Back view.** A sulking companion shows its back sprite, and the user can also turn the back view on permanently.
18. **Companion card (Home).** The card shows the sprite with its animations, name, shiny mark, rarity and type badges, stage and nature, the boost badge, a hover-expanding growth bar, remaining tokens, the status line and the evolution line, which links to the dex.
19. **Celebrations.** Hatch, evolve and Ditto reveal play a flash and pop. A shiny hatch adds a ✨ burst. A rare candy shows "+XP" and a mint shows sparkles.
20. **Notifications.** Hatch, shiny hatch, prayer hatch, evolve, graduate and Ditto reveal each send a system notification, and the latest event also goes to the floating pet.
21. **Localized names.** All PokéAPI languages are stored. Resolution order is the app language, then `en`, then `#id`.
22. **Self-healing persistence.** Decoding is lenient field by field. A corrupt file is moved aside and the newest snapshot is restored. Values are sanitized on every load, and a one-time profile migration runs with a backup.

---

## 3. Rules & constants

### 3.1 Balance (Model:107-179)

| Constant | Value | Source |
|---|---|---|
| `eggHatchThreshold` | `5_000_000` | Model:111 |
| `repeatGrowthMultiplier` | `2` | Model:112 |
| `sulkAfterIdle` | `3 * 3600` s | Model:109 |
| `graduationTotal(common)` | `750_000_000` | Model:116 |
| `graduationTotal(uncommon)` | `1_875_000_000` | Model:117 |
| `graduationTotal(rare)` | `3_000_000_000` | Model:118 |
| `graduationTotal(legendary)` | `6_000_000_000` | Model:119 |
| `difficultyRange` | `0.1...2.0` | Model:136 |
| `defaultDifficulty` | `1.0` | Model:138 |
| slider default snap | ±0.01 of track position | Model:156 |
| `PokemonAssets.animatedSpeciesIDs` | `1...649` | Model:325 |
| `SaveTransfer.maxTokenValue` | `1_000_000_000_000_000` | SaveTransfer.swift:74 |

**Phase threshold** (Model:123-131). `stageIndex` is 0-based and k is `totalForms`:

```swift
let kk = max(1, k); let i = stageIndex + 1
let standard = Int((T * Double(i) / Double(kk*(kk+1))/2).rounded())   // .toNearestOrAwayFromZero
return max(1, Int((Double(standard) / Double(max(1, growthMultiplier))).rounded()))
```

`growthMultiplier` is 2 when `MonState.hasGrowthBoost`, otherwise 1 (Model:487-493). Rounding happens twice: first the standard threshold, then the division.

**The threshold actually used** is `stageThreshold(mon) = scaled(mon.phaseThreshold, by: growthDifficulty)` (Store:169-171). `scaled(base, d) = Int((Double(base) * clampDifficulty(d)).rounded())` (Model:150-152). Swift applies **no** `max(1)` after scaling. Every threshold consumer must go through this one function.

**Egg threshold** is `scaled(5_000_000, growthDifficulty)` (Store:163-165).

**`clampDifficulty`** (Model:142-145): a non-finite value becomes `1.0`, and anything else is clamped to [0.1, 2.0]. Note that 0 and negative values clamp to 0.1. They do **not** reset to 1.

**Slider mapping** (Model:158-178):
- `position → value`: `p = clamp(position, 0, 1)`. If `|p − position(1.0)| < 0.01`, the value is `1.0`. Otherwise the value is `snap(0.1 * pow(20, p))`.
- `value → position`: `log(clamp(v)/0.1) / log(20)`.
- `snap(v)` keeps two significant figures: `m = 10^(floor(log10 v) − 1)`, then `round(v/m)*m`. For v ≤ 0 it returns 0.1.

**Difficulty change** (Store:126-152): `setGrowthDifficulty(new)` clamps the value and does nothing if it is unchanged. Otherwise it rescales the banked progress of the current egg, or of the current stage when there is an active Pokémon, then saves. It never triggers evolution, hatch or graduation. The rescale works like this:

```swift
oldThr = max(1, Int((Double(base)*old).rounded())); newThr = max(1, Int((Double(base)*new).rounded()))
v = floor(credits / oldThr * newThr), clamped to [0, maxTokenValue]
return credits < oldThr ? min(newThr - 1, v) : v    // an incomplete stage stays incomplete
```

Here `base` is `active.phaseThreshold` (rarity, forms and boost, without difficulty) or `eggHatchThreshold`. The difficulty is persisted in **UserDefaults** `growthDifficulty`, not in the save. `shopDifficulty` is stored the same way and only affects prices.

**Launch migration** (Store:89-102): `previousGrowth = isFinite ? min(20, max(0.0001, stored)) : 1`. When this differs from the clamped value, banked growth is rescaled from the old to the new value and saved. Both difficulties are then written back.

Worked examples (difficulty 1.0):
- A common 3-form line costs 125M, 250M and 375M per stage. With the boost it costs 62.5M (62_500_000), 125M and 187.5M.
- A legendary single-form line costs 6B. A rare 2-form line costs 1B and 2B.
- Ditto after the reveal (rare, k=1) costs 3B. With the boost (a repeat Ditto) it costs 1.5B.

### 3.2 Rarity (Model:60-102)

`from(captureRate, isLegendary, isMythical)` works as follows:
- legendary or mythical → `legendary`
- `captureRate ≤ 45` → `rare`
- `≤ 120` → `uncommon`
- otherwise → `common`

`captureRateCeiling` is 45 for rare, 120 for uncommon, 255 for common and `nil` for legendary. `includes(captureRate:)` returns false for legendary. `sortRank` orders the tiers as common 0, uncommon 1, rare 2, legendary 3.

### 3.3 Odds (Model:447-454, 244-250, Store:1721-1736)

| Roll | Rule |
|---|---|
| Shiny | `rng.next() % D == 0`. D is 64, or 48 when `inventory["shinyCharm"] > 0`. The notification text uses the same D. |
| Nature | `PokemonNature.allCases[rng.next() % 25]`. The case order is in Model:400-404. |
| Ditto disguise | Only when `dittoDisguiseRollingEnabled` is on, which defaults to `AppEnv.isBundledApp`. The condition is `rarity == .common && line.totalForms >= 2 && rng.next() % 128 == 0`. `line.totalForms` is `tree.depth` after the animated filter. The disguise is `line.baseID`. Because `&&` short-circuits, no RNG is consumed when the gate is off. |
| Hidden ability | `random.next() % 128 == 0`, and only when the species has hidden abilities (Profile:156). |
| Gender | `genderRate < 0` → genderless. Otherwise `random.next() % 8 < genderRate` → female, else male (Profile:144-150). |
| Mint | Picks a nature uniformly from the 24 that differ from the current one, or from all 25 if the current one is nil (Store:1263-1264). |

**RNG draw order at hatch** (Store:1783-1796): shiny, nature, ditto (gated), the evolution plan (one draw per branching node), the profile seed (`rng.next()`), and finally the form roll if no form was pre-rolled. The draw order only matters for deterministic tests.

### 3.4 Species roll: `chooseBase` (Store:1912-1937)

1. Get the index from `baseSpeciesIndex()`, which returns `[{id, captureRate}]` (see §4.4).
2. If `eggTier` is set, filter to `tier.includes(captureRate)`. If the result is empty, return `nil`: the egg is kept and the roll is retried on the next tick. The roll never falls back to the full pool.
3. Each weight is `CollectionWeight.adjusted(captureRate, isCollected: collectedFinals has prefix "\(id):")`, where `adjusted(w, collected) = collected ? max(1, w/2) : max(1, w)` (CollectionWeight.swift:3-5).
4. Draw once: `r = rng.next() % total`. Walk the list subtracting each weight and pick the first index where `r < 0`.
5. If the index is unavailable, use the **REST fallback** (Store:1942-1963). It makes up to 16 attempts. Each attempt picks a uniform `id ∈ 1...649` and calls `baseSpecies(id)`. That call returns nil for Ditto or when `evolves_from_species != nil`. If the species fails the tier test it is skipped. A network error returns `nil` immediately.

Ditto (#132) is excluded from both paths (API:219, 239).

### 3.5 Form catalog and roll (Form:45-147)

- `catalog[speciesID] = [Entry(rawValue, formID, pokemonID?)]`, with the default form first. Only forms that have a Gen V animated sprite are listed.
  - `formID` is the PokéAPI `pokemon-form` id that carries the names.
  - `pokemonID` is the variety id whose **static** PNG holds the form. When it is nil, the static sprite is named `"<id>-<form>.png"`.
- **Unown (#201), 28 forms:** `a`…`z`, then `exclamation` and `question`. The formID is 201 for `a`, and `10000 + index` for the rest (`b` = 10001 … `z` = 10025, `exclamation` = 10026, `question` = 10027). None has a `pokemonID`. The symbol is the uppercase letter, `!` or `?`.
- Other species (raw names; formID / pokemonID):
  - 351 Castform: normal(351), sunny(10028/10013), rainy(10029/10014), snowy(10030/10015)
  - 386 Deoxys: normal(386), attack(10031/10001), defense(10032/10002), speed(10033/10003)
  - 412 Burmy: plant(412), sandy(10034), trash(10035)
  - 413 Wormadam: plant(413), sandy(10036/10004), trash(10037/10005)
  - 421 Cherrim: overcast(421), sunshine(10038)
  - 422 Shellos: west(422), east(10039) · 423 Gastrodon: west(423), east(10040)
  - 479 Rotom: normal(479), heat(10058/10008), wash(10059/10009), frost(10060/10010), fan(10061/10011), mow(10062/10012)
  - 487 Giratina: altered(487), origin(10063/10007)
  - 492 Shaymin: land(492), sky(10064/10006)
  - 493 Arceus: normal(493), fighting 10045, flying 10047, poison 10052, ground 10050, rock 10054, bug 10041, ghost 10048, steel 10055, fire 10046, water 10056, grass 10049, electric 10044, psychic 10053, ice 10051, dragon 10043, dark 10042, fairy 10085. This is the catalog order, and none has a `pokemonID`.
  - 550 Basculin: red-striped(550), blue-striped(10066/10016)
  - 555 Darmanitan: standard(555), zen(10067/10017)
  - 585 Deerling: spring(585), summer 10068, autumn 10069, winter 10070 · 586 Sawsbuck: spring(586), summer 10071, autumn 10072, winter 10073
  - 641/642/645 Tornadus, Thundurus, Landorus: incarnate, therian (10079/10019, 10080/10020, 10081/10021)
  - 646 Kyurem: normal, black(10082/10022), white(10083/10023)
  - 647 Keldeo: ordinary, resolute(10084/10024)
  - 648 Meloetta: aria, pirouette(10074/10018)
  - 649 Genesect: normal, douse 10075, shock 10076, burn 10077, chill 10078
- `lineBaseIDs = {413:412, 421:420, 423:422, 555:554, 586:585}`. `formSpecies(baseID)` returns the catalog keys whose line base equals `baseID`, sorted. `lineForms(baseID)` is the union of their forms with duplicates removed, default first.
- `resolved(speciesID, form)`: the form itself if that species has it, otherwise the species default. Species without forms resolve to nil. `resolved(baseID, form)` applies the same rule against `lineForms`. A Sandy Burmy therefore becomes a Sandy Wormadam, while Mothim (414) has no form.
- **Roll** (`rollLineForm`, Store:1681-1685): this only runs when `lineForms` is non-empty, so other species keep their RNG sequence. Each weight is `adjusted(2, isCollected)`, which gives **2 for uncollected forms and 1 for collected ones**. The roll is `rng.next() % sum`, walked over `forms.dropLast()`, and the remainder goes to the last form.
- "Collected" for the roll means `collectedLineForms(baseID)` (Model:783-790): a form counts as collected only when **every** species of the line that has that form owns it. That covers the dex (graduated or released) and the active Pokémon's reached stages, shiny or not.
- `unnamedDefaultSpecies = {351, 479, 646, 649}`. The default form of these is labelled with the localized `normalForm` ("Normal Form", "노말폼" …). Unown and these defaults have no name resource.
- Display name (Form:176-181):
  - Unown always shows `"Name [A]"`.
  - Other species add `"Name [Label]"` only for a non-default form with a non-empty label.
  - Evolution-line labels pass `label: nil`, so only Unown's letter appears there (View:535).
- **Legacy keys:** the form is read from `form` first and then `unownForm`. The same pattern applies to `pendingForm`/`pendingUnownForm` and `representativeForm`/`representativeUnownForm` (Form:194-202). A malformed value decodes as nil and then resolves to the default.

### 3.6 Hatch flow (Store:1560-1824)

Each `update` tick runs these checks in order:

1. If `active == nil`, `installBaselineSet` is true and no hatch is running: when `eggUsage >= eggThreshold`, run `hatchIfNeeded()`. Otherwise run `ensureEggPrefetch()`.
2. **Prefetch** (Store:1628-1678):
   - If there is no `pendingHatchID`, call `chooseBase()` and persist `pendingHatchID` and `pendingForm = rollLineForm(id)`.
   - Fetch the line (`provider.line`), then, in bundled builds only, warm these sprites: static plain, animated plain, animated shiny, and static shiny if the species has forms. Also warm the form names.
   - Then set `prefetchedLineID = id`.
   - A failure at any step marks `isHatchRetryDelayed` if the egg is ready, and the next tick resumes from that step.
   - If the egg became ready while prefetching, the hatch fires once prefetch finishes.
3. **`hatchIfNeeded`**: if prefetch is mid-roll (`pendingHatchID == nil && prefetchInFlight`), return. Otherwise take the lock, use `pendingHatchID` or call `chooseBase()`, and if the result is nil set `isHatchRetryDelayed = true`. Then run `hatchCore`.
4. **`hatchCore`**:
   1. Fetch the line. On failure, keep the egg and mark the retry.
   2. **Generation guard:** if the active subject changed, for example after a save import, discard the result.
   3. **Tier gate:** if `eggTier` is set and `line.rarity.sortRank < tier.sortRank`, clear the pending species, form and prefetch state, keep the egg, and re-roll on the next tick.
   4. Compute `overflow = max(0, eggUsage − eggThreshold)`. Set `eggUsage = 0` and `eggTier = nil`, move `eggPrayers` onto the Pokémon, and reset the egg prayers.
   5. Roll shiny, nature, ditto, plan, profile and form, in that order.
   6. Set `hasGrowthBoost = hasCollectedFinal(forBaseID: line.baseID)`.
   7. Create the `MonState`: `pathIDs = [base]`, `plannedPathIDs = plan`, `stageIndex = 0`, `usedAtStage = 0`, `totalForms = plan.count`, profile from `generate(seed)` (enriched if details are cached).
   8. Send the notification:
      - title `notifShinyHatchTitle` or `notifHatchTitle`
      - body `notifPrayerHatchBody(name, prayers, shiny, rarity)` if there were prayers, otherwise `notifShinyHatchBody(name, odds: D)` or `notifHatchBody(name)`
      - while disguised as Ditto, `showShiny` is false
   9. Set `justEvolvedTo = nil`, `displayState = .levelUp`, and `eventUntil = now + 4s`.
   10. Apply the overflow with `applyUsage(overflow)`, which may evolve the Pokémon at once.
   11. If the Pokémon is still active, fire `Celebration.hatch(shiny: showShiny)` **after** the overflow evolutions, so a shiny burst is not overwritten. Save, then load details.

### 3.7 Evolution and graduation: `applyUsage` (Store:1002-1059)

```text
active.usedAtStage += delta; reconcile profile growth
if currentLine == nil → save and return   // credit is never dropped; evolution waits for the line
loop (max 50 iterations):
  thr = stageThreshold(a); break if usedAtStage < thr
  node = tree.node(currentID) else break
  if disguised Ditto (not revealed) → start revealDitto() and break   // checked BEFORE graduation
  if node is a leaf → graduate(); break
  next = the planned child at plannedPathIDs[stageIndex+1] if it is a child of this node,
         else pickPlannedChild + repairedPlan (logs "evolve: repaired invalid planned path")
  pathIDs = pathIDs.prefix(stageIndex+1) + [next]; stageIndex += 1; usedAtStage -= thr (overflow carries)
  enrich the profile with cached details or load them; justEvolvedTo = new name;
  fire Celebration.evolve; eventUntil = now + 4s; notify(notifEvolveTitle, notifEvolveBody(from:old,to:new))
reconcile profile growth; save
```

- **`pickPlannedChild(node, baseID)`**: the pool is the children with at least one final ID not yet in `collectedFinals` as `"\(baseID):\(final)"`. If no child qualifies, the pool is all children. Pick uniformly with `rng.next() % count`. `makeEvolutionPlan(root)` repeats this down to a leaf.
- **`repairedPlan(realized, stageIndex, fallbackRoute)`**: take `prefix = realized[0...min(stageIndex, count−1)]`. If `fallbackRoute.first == prefix.last`, the result is `prefix + fallbackRoute.dropFirst()`. Otherwise it is `prefix`.
- **Line normalization on load** (`normalizedEvolutionState`, Store:1102-1121): `realized` is the longest valid path of `pathIDs` from the root. The saved plan is reused only if it is fully valid, starts with `realized` and ends at a leaf. Otherwise a new suffix is planned from the realized tip. The function then sets `stageIndex = realized.count − 1` and `totalForms = plan.count`. Save immediately, then call `applyUsage(0)`.
- **`graduate()`** (Store:1123-1154):
  1. Advance the profile growth to `graduationTotal` and enrich it.
  2. Insert `"\(baseID):\(finalID)"` into `collectedFinals`.
  3. Append a `DexEntry` with these fields: `id = profile.instanceID ?? UUID`, `chainOrder = pathIDs`, `caughtAt = now`, the **raw** `isShiny`, `names` restricted to `pathIDs`, plus `form` and `prayers`.
  4. Set `justGraduated = name` and send `notifGraduateTitle` / `notifGraduateBody(name)`. Set `eventUntil = now + 6s`.
  5. Set `active = nil`, reconcile the representative, bump the generation, set `currentLine = nil`, `eggUsage = 0`, reset the prayers and clear `isHatchRetryDelayed`. Prefetch the next egg immediately.
- **Release** (`buyEgg`, Store:1378-1402; also `releasedDexEntry`, Store:355-375):
  1. Charge the price.
  2. Append a DexEntry built from **reached forms only**: `pathIDs.prefix(max(1, stageIndex+1))`, with `finalID = last`, `caughtAt = releasedAt = now`, `isShiny = displaysShiny` and `id = profile.instanceID`.
  3. Leave `collectedFinals` **unchanged**.
  4. Set `active = nil` and reconcile the representative. Bump the generation and clear the line. Set `eggUsage = 0`, reset the prayers, set `eggTier = tier`, clear the pending species and form, and clear `prefetchedLineID` and the event flags. Prefetch, then save.
  5. The purchase requires an active Pokémon and a tier in `FreshEgg.shopTiers = [nil, .uncommon, .rare]`.

### 3.8 Ditto disguise and reveal (Store:1828-1878)

- `displaysShiny = isShiny && (dittoDisguise == nil || dittoRevealed)` (Model:484). **Every** display path uses this rule: header, dex synthesis, representative, catch log and release record.
- While disguised, the `MonState` looks exactly like the disguise species, including its rarity, which is always common. Rare-candy planning previews the apparent path (Store:1177-1238).
- **Trigger:** `usedAtStage >= stageThreshold` while disguised and unrevealed. This is checked in `applyUsage` before graduation, and again on every `update` tick as a backup.
- **Reveal:**
  1. Load line 132. On failure, retry on the next tick.
  2. Re-check the generation and the threshold.
  3. Set `carryOver = usedAtStage − firstThreshold`.
  4. Set `baseID = 132`, `pathIDs = [132]` and `plannedPathIDs = makeEvolutionPlan(dittoLine.tree)`, which is `[132]`. Set `stageIndex = 0`, `rarity = dittoLine.rarity` (capture rate 35, so rare), `totalForms = plan.count`, `usedAtStage = carryOver`, `dittoRevealed = true` and `form = nil`.
  5. Set `hasGrowthBoost = hasCollectedFinal(132)`.
  6. Call `profile.rebaseForSpeciesIdentity(from: oldRarity, to: rare)`.
  7. Keep `isShiny`, `nature` and `dittoDisguise`. The `dittoDisguise` field keeps the disguise species ID so it can be shown later.
  8. Reconcile the representative.
  9. Fire `Celebration.dittoReveal(shiny: isShiny)` and set `.levelUp` with `eventUntil = now + 5s`.
  10. Notify with `notifDittoRevealTitle`/`Body(disguiseName)`, or the shiny variants. Save, call `applyUsage(0)`, then load the Ditto details.
- `rebaseForSpeciesIdentity` (Profile:130-140): `fraction = min(1, growthTokens / T_old)`, then `growthTokens = floor(fraction × T_new)`. It clears gender, ability and moves, so they are re-rolled on the next enrichment, and then calls `advanceGrowth`. Because of the high-water mark the level never drops.

### 3.9 Individual profile (Profile:92-296)

- `generate(seed)` uses `ProfileRNG`, which is SplitMix64:

  ```text
  state = seed == 0 ? 0x9E3779B97F4A7C15 : seed
  next: state += 0x9E3779B97F4A7C15; z = state
        z = (z ^ z>>30) * 0xBF58476D1CE4E5B9; z = (z ^ z>>27) * 0x94D049BB133111EB; return z ^ z>>31
  ```

  The IVs are drawn in the order hp, attack, defense, specialAttack, specialDefense, speed, each as `next() % 32`. The other initial values are `level = 5`, `gender/ability = nil`, `moves = []` and `instanceID = UUID`.
- `enrich(details)` uses a separate RNG seeded with `seed ^ 0xA11B1E5D9EED`:
  1. If gender is nil, roll it as in §3.3.
  2. If `abilitySlot` is nil:
     - If hidden abilities exist and `next() % 128 == 0`, pick from `hidden[next() % count]` sorted by slot.
     - Otherwise pick from `normal[next() % count]` sorted by slot.
     - Otherwise use the lowest slot.
  3. Resolve the chosen ability: match on slot and hidden flag, then on slot alone, then fall back to the first normal ability, then the first hidden one. Write back its name, slot and hidden flag.
  4. Set `moves` to the **last 4** of `details.levelUpMoves(through: level)`. That list is built from moves whose learn methods include `method == "level-up"` with `level ≤ L`, using the minimum level per move. It is sorted by `(learnedAtLevel, name)`.
- **Learnset source:** only version group `black-2-white-2` is kept (Profile:21, API:86-96).
- `advanceGrowth(to:)`:

  ```text
  growthTokens = min(maxTokenValue, max(growthTokens, max(0, candidate)))
  level = min(100, max(level, max(5, 5 + floor(min(1, growthTokens/T) * 95))))
  ```

  This is a high-water mark: difficulty changes and imports never lower the level.
- **Active growth reconciliation** (Store:2250-2264), run on every `applyUsage`:

  ```text
  completed = Σ_{s < stageIndex} phaseThreshold(rarity, totalForms, s)   // standard: no boost, no difficulty
  standardPhase = phaseThreshold(rarity, totalForms, stageIndex)
  fraction = clamp(usedAtStage / max(1, stageThreshold(active)), 0, 1)
  candidate = min(T, completed + floor(standardPhase * fraction))
  ```

  Re-enrich the profile if details are cached.
- **Stats** (Profile:225-240):
  - Order: hp, attack, defense, special-attack, special-defense, speed.
  - HP: `((2*base + iv) * L) / 100 + L + 10`, using integer division. **Shedinja (#292) always has HP 1.**
  - Other stats: `floor((((2*base + iv) * L) / 100 + 5) * natureMod)`, where `natureMod` is 1.1 for the raised stat, 0.9 for the lowered one and 1 otherwise. The 25-nature table is in Profile:245-274. Hardy, docile, serious, bashful and quirky are neutral.
  - There are no EVs.
- **Display scale:** `displayScaleMaximum = max(300, maxValue)` rounded up to the next multiple of 100.
- **`sanitize()`:** clamp level to 5…100, growth to 0…max and IVs to 0…31. Keep only the first 4 moves, with names cut to 80 characters and levels clamped to 0…100. Replace an empty `instanceID` with a UUID.
- **Migration** (Store:2201-2246): this runs once for profiles that are nil.
  - Active Pokémon: seed = `FNV-1a-64("active:\(baseID):\(pathIDs joined ",") :\(lastDate)")`. There is no space before the second colon in the real string: `"active:%d:%s:%s"`.
  - Dex entries: seed = `FNV-1a-64("dex:\(entry.id):\(finalID)")`, with `instanceID = entry.id`. Growth is T for graduated entries. For released entries it is `reconstructedGrowthTokens(rarity, totalForms: chain.count, completedStages: chain.count − 1)`.
  - FNV-1a uses offset basis `0xcbf29ce484222325` and prime `0x100000001B3`, over UTF-8 bytes with wrapping multiplication.
  - Before the first migrating save, copy the state file to `companion-state.pre-profiles-v1.json`, once and without overwriting.

### 3.10 Display state (Store:1987-2001, 1974-1985)

The first matching rule wins:

1. `active == nil` → `egg`
2. `justGraduated != nil || now < eventUntil` → `levelUp`
3. `limitWarning`, meaning any tracked limit window is at or above the critical threshold (`UsageStore.isLimitWarning`) → `tired`
4. `hasUsageData && idle >= 3h` → `sulk`
5. `!hasUsageData || todayTokens == 0` → `sleep`
6. Otherwise the burn tier decides:
   - `idle` → `idle`
   - `normal` → `working`
   - `fast` or `blazing` → `focus`

The burn tier is the summed `activeBlock.tokensPerMinute` across providers: ≤1000 is `idle`, under 100k is `normal`, under 400k is `fast`, and anything higher is `blazing` (UsageStore.swift:878-884).

**Effective idle:**
- The input is `timeSinceLastUsage`: now − the newest provider `lastUsage`. It is nil before the first complete refresh, and 7 days when no provider reports a time.
- The store caps it with its own observation. When today's total rises on the same date, `lastObservedUsage = now`, and idle becomes `min(reported, now − lastObservedUsage)`. A nil idle never counts as idle.

**Event windows:**
- The `levelUp` window lasts 4s for hatch and evolve, 5s for the Ditto reveal and 6s for graduation.
- `justGraduated` and `justEvolvedTo` are cleared **only** when `eventUntil` expires on an update tick (Store:968-970).

**Back view:**
- `facesBack = active != nil && (alwaysFacesBack || displayState == .sulk)`.
- `alwaysFacesBack` is stored in UserDefaults under `alwaysFacesBack` (default false). It can be toggled from Settings, the status item menu or the pet menu.

**Status lines** (View:912-924):
- `levelUp` shows `statusEvolved(justEvolvedTo)` if set, otherwise `statusGrew`.

| State | en | ko |
|---|---|---|
| egg | "Hatching soon." | "곧 깨어나요." |
| idle | "Keeping quiet today." | "오늘은 조용히 자리를 지켜요." |
| working | "Today's work is piling up." | "오늘의 작업 흔적이 쌓이고 있어요." |
| focus | "In focus mode now." | "지금은 집중 모드예요." |
| tired | "Close to the limit. A short break is fine." | "한도에 가까워요. 잠깐 쉬어도 괜찮아요." |
| sleep | "Sleeping now." | "지금은 자고 있어요." |
| sulk | "Sulking with its back turned." | "삐져서 등을 돌리고 있어요." |
| evolved | "Evolved into {name}!" | "{name}(으)로 진화했어요!" (uses the Korean direction particle) |
| grew | "It grew!" | "성장했어요!" |

The ja, es, fr, pt, de and ru strings are in Localization.swift:1350-1360.

### 3.11 Ledger crediting per provider (Store:867-997)

The inputs are `todayTokensByProvider` (only providers whose snapshot is dated today), `todayDate` (a local date key), `hasUsageData`, and the other display inputs. Define `hasCurrentProviderData = hasUsageData && !map.isEmpty`.

- **Baseline not set:**
  - Without data, show egg or idle, start loading the line and return. No baseline is set yet.
  - With data, set `installBaselineSet = true`, `claimedTodayTokensByProvider = map` and `lastDate = date`. No credit is given and the state is saved.
- **Baseline set, with data:**
  - `claimed == nil` (a legacy save): seed it with the map and `lastDate` and credit nothing. The legacy aggregate key `claimedTodayTokens` is deliberately ignored.
  - **Date changed:**
    1. Set `lastDate = date`.
    2. Build a new ledger: every provider **already known** gets 0, then each current provider gets its current value.
    3. Credit the **sum of the current map**.

    Providers that are known but missing in this refresh keep a 0 baseline. When they report later that day, their full day is credited.
  - **Same date:** for each provider in the map:
    - Unknown provider: seed it and credit nothing.
    - `current < previous`: rebase that provider only and log a regression line.
    - Otherwise credit `current − previous` and store `current`.

    Providers missing from the map are left untouched.
- **Baseline set, without data:** the ledger is not touched.

A positive delta goes to `usedSinceInstall += delta`. When there is no active Pokémon it also goes to `eggUsage += delta`; otherwise it goes through `applyUsage(delta)`. Rare-candy XP goes **only** through `applyUsage`: it never reaches `usedSinceInstall` and is not scaled by difficulty. `RareCandy.xp` is 100_000_000.

### 3.12 Representative species (Model:752-821, Store:201-248)

- `representativeSpeciesID` and `representativeForm` are stored in the save. Nil means the menu bar and pet follow the active Pokémon or the egg.
- `setRepresentativeSpeciesID(id, form)` refuses (returns false) unless `ownsSpecies(id, form)`. Ownership means one of:
  - some dex entry's `chainOrder` contains the ID and its resolved form matches
  - or the active Pokémon's `pathIDs.prefix(stageIndex+1)` contains it with a matching form

  The planned path is **never** used for this.
- `reconcileRepresentativeSelection()` resets the pin to nil when the species is no longer owned. It runs after graduation, release, the Ditto reveal, line normalization, sanitize and set calls.
- `RepresentativeSubject` contains:
  - `speciesID`
  - `isShiny`: `ownsShinySpecies(id, form)` for a pinned species, which honours `displaysShiny`. When following the active Pokémon it is `currentIsShiny`.
  - the resolved `form`

  It is recomputed only inside `save()`.
- `menuGrowthText` always follows the companion being raised, not the representative:
  - `percent = min(max(0, credits), total) * 100 / total`, using integer floor.
  - The text is `"Hatch N%"` for an egg and `"Growth N%"` otherwise (Localization.swift:348).

### 3.13 Names (Names:5-37, Model:13-36)

- **`collect`:** language code = lowercase(trim(code)) with `_` replaced by `-`. Empty codes and empty names are skipped. All languages are kept.
- **`resolve(names, preferred)`:** normalize every key with the same rule, then try each preferred code followed by `"en"`. If nothing matches, return nil. The caller then falls back to `"#\(id)"`.
- **API codes per language:**
  - ko → ko
  - en → en
  - ja → ja-hrkt, then ja
  - es → es
  - fr → fr
  - pt → pt-br, then pt
  - de → de
  - ru → ru (PokéAPI has no ru, so it falls back to en)
- **`identifier("sky-forme")`** returns "Sky Forme". It is the offline fallback for form, type, move and ability names.
- **System default language:** the first two letters of `Locale.preferredLanguages.first` if they match one of the 8 languages, otherwise en.
- The egg's display name is the literal, unlocalized `"Token Egg"` (Store:259).

### 3.14 TokenFormatter.compact (TokenFormatter.swift:5-19)

| Range of v | Output |
|---|---|
| `< 1000` | the plain integer |
| `< 999_950` | `v/1e3`, 1 decimal, trailing zeros trimmed, then "K" |
| `< 999_950_000` | `v/1e6`, 1 decimal, then "M" |
| otherwise | `v/1e9`, 2 decimals, then "B" |

A negative value gets a `-` prefix. Examples: 400000 → "400K", 62_500_000 → "62.5M", 1_875_000_000 → "1.88B".

---

## 4. Data and persistence

### 4.1 Files (all under `AppStatePaths.directory()`, which is `~/Library/Application Support/PokeTokenBar` or `$PTB_STATE_DIR`)

| File | Content |
|---|---|
| `companion-state.json` | `CompanionState` written with the default JSONEncoder, atomic write (Store:2306-2311) |
| `companion-state.json.corrupt` | the original when decoding fails completely. The newest valid snapshot is then loaded, or the state starts fresh (Store:2278-2300) |
| `companion-state.pre-profiles-v1.json` | the one-time backup taken before profile migration |
| `companion-state.pre-import-<yyyy-MM-dd-HHmmss>.json` | taken before an import; the newest 5 are kept |
| `companion-snapshot-*` | automatic snapshots every 12h when the state is non-empty; the newest 10 are kept (SaveSnapshots.swift:16-18) |
| `pokemon-details-v1/<id>.json` | `{fetchedAt, details}` with a 30-day TTL. When stale and offline, the old copy is still used |
| `pokemon-names-v1/<kind>-<name>.json` | `{fetchedAt, names}` with a 30-day TTL. `kind` is type, ability, move or pokemon-form |
| `base-index.json` | `{fetchedAt, entries:[{id, captureRate}]}` with a 30-day TTL. It is **always** in `~/Library/Application Support/PokeTokenBar` and ignores `PTB_STATE_DIR` (API:126-131) |
| `sprites/<cacheKey>.<gif\|png>`, `sprites/item-<name>.png`, `sprites/egg.png` | the sprite cache. It also ignores `PTB_STATE_DIR` (Sprite:19-20) |

**Encoding notes, important for any cross-platform import:**
- `Date` uses the Swift default `.deferredToDate`, which is a **Double of seconds since 2001-01-01T00:00:00Z** (add 978307200 to get Unix time).
- `[Int: X]` dictionaries encode as JSON objects with string keys (`"25": {...}`).
- `Set<String>` encodes as an array.
- Nil optionals are **omitted**.
- `PokemonForm` encodes as a bare string.
- `UInt64` seeds can exceed 2^53.

### 4.2 `CompanionState` keys (Model:652-748)

All fields decode leniently: a missing key or a type mismatch falls back to the default.

| Key | Type | Default | Notes |
|---|---|---|---|
| `installBaselineSet` | bool | false | |
| `usedSinceInstall` | int | 0 | the lifetime growth meter |
| `spentTokens` | int | 0 | the wallet ledger (shop) |
| `eggUsage` | int | 0 | growth for the current egg |
| `eggTier` | `"common"\|"uncommon"\|"rare"\|"legendary"`? | nil | an unknown value decodes as nil. Sanitize drops `legendary` and drops the field whenever there is an active Pokémon |
| `pendingHatchID` | int? | nil | the pre-rolled species |
| `pendingForm` (legacy `pendingUnownForm`) | string? | nil | resolved against `pendingHatchID` |
| `eggPrayers` | `{count:int}`? | nil | cosmetic only |
| `claimedTodayTokensByProvider` | `{providerID:int}`? | nil | **absent and empty mean different things** (§3.11) |
| `lastDate` | string | "" | the local day key |
| `active` | MonState? | nil | if corrupt, decodes as nil (the egg) while the rest is kept |
| `representativeSpeciesID` | int? | nil | |
| `representativeForm` (legacy `representativeUnownForm`) | string? | nil | |
| `dex` | [DexEntry] | [] | **each entry decodes in isolation** (a bad entry is dropped) |
| `collectedFinals` | [string] (set) | [] | `"base:final"` |
| `language` | `ko\|en\|ja\|es\|fr\|pt\|de\|ru` | system default | an import keeps the device's language |
| `inventory` | `{"rareCandy"\|"mint"\|"shinyCharm": int}` | {} | |
| `candyGrantTier`, `candyWindowEpoch`, `candyFeatureSeeded` | | | candy grants (other area) |
| `notifiedDexCompletions`, `dexCompletionsSeeded` | | | dex badges (other area) |

**`MonState` fields:** `baseID`, `pathIDs` (must be non-empty or the decode throws), `plannedPathIDs` (empty or missing falls back to pathIDs), `stageIndex` (clamped to the path), `usedAtStage`, `rarity`, `totalForms`, `isShiny`=false, `nature`?, `form`? (legacy `unownForm`), `profile`? (a corrupt profile becomes nil and is re-migrated), `hasGrowthBoost`=false, `dittoDisguise`?, `dittoRevealed`=false, `prayers`?

**`DexEntry` fields:** `id` (a UUID string, or the active Pokémon's `instanceID`), `baseID`, `finalID`, `chainOrder`, `rarity`, `caughtAt`? (Date), `isShiny`, `nature`?, `form`? (legacy `unownForm`), `profile`?, `prayers`?, `names`? (`{speciesID: {lang: name}}`; a legacy flat map decodes as nil), `namesVersion`? (current 1; set when every chain species has names), `releasedAt`? (nil means graduated)

**`PokemonProfile` fields:** `instanceID`, `seed` (u64), `gender`? (`male|female|genderless`), `ivs{hp,attack,defense,specialAttack,specialDefense,speed}`, `abilitySlot`?, `abilityName`?, `abilityIsHidden`, `level`, `growthTokens`, `moves[{name, learnedAtLevel}]`

**`PokemonDetails` cache fields:** `speciesID`, `name`, `height` (decimetres), `weight` (hectograms), `baseExperience`?, `genderRate` (−1 means genderless), `types` (sorted by slot), `baseStats{statName:int}`, `abilities[{name,slot,isHidden}]` (sorted by slot), `moves[{name, learnMethods[{method, level}]}]` (BW2 only, sorted by name)

### 4.3 Sanitization on every load and import (SaveTransfer.swift:143-190)

- Clamp every token field to 0…1e15.
- If there is an active Pokémon, set `eggTier`, `pendingHatchID` and `eggPrayers` to nil.
- If `eggTier` has no capture ceiling (legendary), set it to nil.
- Resolve `pendingForm`.
- **For the active Pokémon:**
  - Clamp `totalForms` to 1…12 and `stageIndex` to the path.
  - Deduplicate `pathIDs` and `plannedPathIDs`, keeping first occurrences, and re-locate `stageIndex` at the current ID.
  - Sanitize the profile.
- **For the dex:** deduplicate `chainOrder` and sanitize profiles.
- Reconcile the representative.

### 4.4 PokéAPI (API:26-269, Names:56-120)

| Call | URL | Notes |
|---|---|---|
| Base index | `POST https://graphql.pokeapi.co/v1beta2`, `Content-Type: application/json`, body `{"query": "{ pokemonspecies(where: {evolves_from_species_id: {_is_null: true}, id: {_lte: 649, _neq: 132}}, order_by: {id: asc}) { id capture_rate } }"}` | 15s timeout. A non-200 response or an empty list is an error. Order: memory, then disk (30 days), then fetch, then stale disk. If everything fails, one REST index build runs in the background per session (batches of 6 IDs over 1…649; it is persisted only when it finds at least 150 bases) |
| Species | `GET https://pokeapi.co/api/v2/pokemon-species/{id}` | Memory cache per session. Uses `capture_rate, is_legendary, is_mythical, names[], evolution_chain.url, evolves_from_species, gender_rate` |
| Chain | the `evolution_chain.url`, **validated**: scheme `https` and host exactly `pokeapi.co`, otherwise `URLError.badURL` | `EvoNode` from `chain` with the ID parsed from the last path segment. The tree is the **whole chain** filtered to IDs ≤ 649 (any child above 649 and its descendants are dropped; if the root is above 649, the tree becomes the base alone) |
| Line names | `pokemon-species/{id}` for every node | `names[id] = collect(species.names)` |
| Details | `GET /api/v2/pokemon/{id}` + species (gender_rate) | Memory, then disk (30 days), then fetch, then stale disk |
| Localized names | `GET https://pokeapi.co/api/v2/{type\|ability\|move\|pokemon-form}/{name}/` | 15s timeout, must be 200. For `pokemon-form` it uses `form_names`, otherwise `names`. Resource names must match `[a-z0-9-]+`. Requests for the same resource are deduplicated. Each species line is cached per session (`lineCache`) |

No custom headers are sent apart from `Content-Type` on GraphQL. There is no User-Agent override.

### 4.5 Sprites (Sprite:3-351)

- **Base URL:** `https://raw.githubusercontent.com/PokeAPI/sprites/master/sprites/pokemon`
- **URL pattern:** `{base}/{animated ? "versions/generation-v/black-white/animated/" : ""}{back ? "back/" : ""}{shiny ? "shiny/" : ""}{name}.{gif|png}`
  - For animated sprites, `name` is `assetName`: `"<id>"` for the default form, otherwise `"<id>-<form>"`.
  - For static sprites, `name` is `staticAssetName`: the variety `pokemonID` if there is one, otherwise the same as `assetName`.
  - Examples:
    - `…/animated/shiny/25.gif`
    - `…/animated/back/shiny/25.gif`
    - `…/animated/201-b.gif`
    - `…/pokemon/10006.png` (Sky Shaymin, static)
    - `…/pokemon/412-sandy.png`
- **Egg:** `{base}/egg.png`. It is static only, cropped to its non-transparent bounds (alpha > 0.01), squared on the longer side and centred, then memoized.
- **Items:** `https://raw.githubusercontent.com/PokeAPI/sprites/master/sprites/items/{rare-candy|shiny-charm}.png`. Mint has no sprite and uses the 🌿 emoji.
- **Cache key:** `"\(assetName)-\(back ? "back-" : "")\(shiny ? "sh" : "")\(animated ? "a" : "s")"`, for example `25-a`, `25-sha`, `25-back-sha` or `201-b-s`. Default forms keep their pre-form keys.
- Animated sprites exist only for IDs 1…649, and no request is made outside that range.
- **Memory LRU:** 64 raw byte entries. The NSImage cache holds 64 entries and the decoded-GIF cache holds 16.
- **Fallbacks:**
  - A missing shiny sprite falls back to the plain one.
  - A missing back sprite falls back to the front one, but **only after a 404**. A 404 is remembered for the session; any other failure keeps retrying the back sprite.
  - A missing animated sprite falls back to the static one.
  - With no image at all, the 🥚 glyph is shown at `size × 0.62`.
- **Animation recovery:**
  - Keep showing the static image and retry the GIF every 2s, doubling up to a 30s cap, with no attempt limit.
  - A GIF with fewer than 2 frames is treated as static.
  - The static placeholder in animated views is cropped to its content, because static PNGs have a 96px padded canvas while GIFs are tightly framed.
- **Frame-rate cap** (`capFrameRate`, SpriteAnimation.swift:31-51): used only for the always-visible surfaces (menu bar and pet). Frames are decimated, not slowed: consecutive delays are accumulated until they reach the floor, and the first frame of each run is emitted. The popover uses native timing.
- **Fit:** aspect-fit inside a square box using `SpriteFit.size`, with nearest-neighbour interpolation. GIF canvases are not square (Wobbuffet's is 36×66).

---

## 5. UI spec: the companion card (View:664-925)

The popover is 360pt wide with 14pt padding, so content is 332pt wide. The scroll content inside it is 320pt (`PopoverMetrics`, PopoverView.swift:8-19). On macOS, `callout` is 12pt and `caption2` is 10pt.

### 5.1 Component tree

```
VStack(alignment: leading, spacing 6)
├─ HStack(alignment: center, spacing 12)
│  ├─ SpriteView 76×76 (bob, animated, shiny=displaysShiny, form, back=facesBack)
│  │    bg secondary @6% · rounded 12 · egg wiggle · celebration scale/flash · overlays · DexEntryLink(r12)
│  ├─ VStack(alignment: leading, spacing 4)
│  │  ├─ HStack(spacing 6): name (callout semibold) · "✨"(11pt, if displaysShiny) · rarity Badge (UPPERCASE)
│  │  ├─ [active ∧ details loaded] TypeBadges(types) · [non-default form] "Data uses the default form" (caption2 secondary)
│  │  ├─ ACTIVE:
│  │  │   HStack(spacing 5): "{stageText}{ · nature}" (caption2 secondary) · [boost] Badge "2× growth" tinted orange
│  │  │   LabeledProgressBar(progress, "X / Y · Z%")
│  │  │   [tokensToNext > 0] "{amount} to next evolution" | "{amount} to graduation" (caption2 tertiary)
│  │  ├─ EGG:
│  │  │   HStack(spacing 6): "🥚 Incubating" (caption2 secondary) | "About to hatch!" (orange, when ≥ 90%) · [eggTier] Badge "{Rarity} or better"
│  │  │   LabeledProgressBar(eggProgress, "X / Y · Z%")
│  │  │   [retry delayed] "⏳ Hatching is delayed — retrying on the next refresh" (caption2 orange, wraps)
│  │  │   else "{amount} to hatch" (caption2 tertiary)
│  │  │   [eggUsage == 0] first-run hint "Grows from your local AI coding usage. Your egg hatches after ~{eggThreshold} tokens." (caption2 tertiary, wraps)
│  │  └─ status line (caption2 secondary)
│  └─ Spacer
├─ [active ∧ lineNodes non-empty] EvoLineView(thumb 40, maxWidth 320, shiny, form, dex links)
└─ [justGraduated] "{name} graduated → saved to the dex. A new Token Egg has arrived!" (caption2 orange)
```

- **Stage text:** "Final form" when the current tree node is a leaf, otherwise "Stage {i} / {k}" with `i = stageIndex+1` and `k = totalForms`. The nature is appended as `" · {localized nature}"`.
- **Egg imminent:** `isEgg && eggProgress >= 0.9`.
- **Data note:** ko "정보는 기본 폼 기준", en "Data uses the default form".
- The screenshot `assets/screenshot-home.png` shows the egg state in dark mode: title "Token Egg", "About to hatch!" in orange, a full-width orange bar, "400K to hatch" in tertiary and "Hatching soon." in secondary.

### 5.2 Colours (macOS system colours; give both modes)

| Token | Light | Dark |
|---|---|---|
| rarity common = `.gray` (systemGray) | #8E8E93 | #98989D |
| rarity uncommon = `.green` | #28CD41 | #32D74B |
| rarity rare = `.blue` | #007AFF | #0A84FF |
| rarity legendary = `.orange` | #FF9500 | #FF9F0A |
| progress fill / egg imminent / boost / candy XP = `.orange` | #FF9500 | #FF9F0A |
| progress track = secondary @20% | rgba(60,60,67,.6)×0.2 ≈ #0000001A | ≈ #FFFFFF1F |
| sprite tile bg = secondary @6% | ≈ #00000008 | ≈ #FFFFFF0A |
| primary text | #000000D9 | #FFFFFFD9 |
| secondary text | #00000080 | #FFFFFF8C |
| tertiary text | #00000040 | #FFFFFF40 |
| current-stage dot = accentColor | #007AFF | #0A84FF |

**Type colours** (Badge.swift:11-31) are the same in both modes:

| Type | Hex | Type | Hex | Type | Hex |
|---|---|---|---|---|---|
| normal | #AAAA99 | fire | #FF4422 | water | #3399FF |
| electric | #FFCC33 | grass | #77CC55 | ice | #66CCFF |
| fighting | #BB5544 | poison | #AA5599 | ground | #DDBB55 |
| flying | #8899FF | psychic | #FF5599 | bug | #AABB22 |
| rock | #BBAA66 | ghost | #6666BB | dragon | #7766EE |
| dark | #775544 | steel | #AAAABB | fairy | #EE99EE |

Any other type is gray.

### 5.3 Badge (Badge.swift:62-110)

- Font: 8pt bold. Padding: 5pt horizontal, 1pt vertical. Shape: capsule.
- `.solid` style: white text on the tint. Type badges add a text shadow of `rgba(0,0,0,.35)` with radius 0.5 and y offset 0.5.
- `.tinted` style: tint-coloured text on the tint at 14% opacity.
- TypeBadges sit in an HStack with spacing 5. Each shows the localized type name in uppercase, from `pokemon-names-v1/type-<name>`. While loading it shows `…`, and on failure the identifier.
- The rarity label is uppercased:

| Rarity | ko | en | ja |
|---|---|---|---|
| common | 일반 | Common | ノーマル |
| uncommon | 고급 | Uncommon | アンコモン |
| rare | 희귀 | Rare | レア |
| legendary | 전설 | Legendary | 伝説 |

### 5.4 LabeledProgressBar (View:556-602)

- Capsule track and capsule fill whose width is `width × clamp(value, 0, 1)`.
- Height is 6pt collapsed and 12pt expanded. The outer frame always reserves 12pt, so hovering never shifts the layout.
- **Hover in:** after 300ms, expand with a spring (response 0.3, damping 0.55). The label (9pt semibold, monospaced digits, primary colour, 1 line, minimum scale 0.7, 6pt horizontal padding) fades in with easeIn 0.12s after a 0.12s delay.
- **Hover out:** cancel the pending expand, collapse with easeOut 0.18s and hide the label in 0.06s.
- **Label:** `"{compact(min(used,total))} / {compact(total)} · {floor(used*100/total)}%"`.
- **Accessibility:** the value equals the label.

### 5.5 Sprite tile animations (View:701-749, 850-910)

- **Bob:** a −3pt y offset, easeInOut 0.9s, repeating forever. It applies only while no GIF frames are playing.
- **Egg wiggle** (imminent): rotation alternates between +5° and −5°, easeInOut 0.35s, repeating forever.
- **Celebration** (played once per `celebrationSeq` and consumed afterwards):
  - A white overlay starts at opacity 0.85 and fades to 0 with easeOut 0.8s.
  - The tile scales from 0.6 to 1 with a spring (response 0.5, damping 0.55).
  - A shiny hatch adds a ✨ at 22pt, top-trailing, offset (+6, −6). It appears with a spring (0.4, 0.5) after 0.3s and is removed after 2.6s with a 0.5s fade.
  - The Ditto reveal adds a 🎭 at 26pt, top, offset y −12, after 0.25s. A shiny Ditto also gets the ✨ after 0.45s. Both are removed after 2.6s.
- **Candy:** a "+{compact(xp)} XP" label (caption, bold, orange) on a regularMaterial capsule with 6/2 padding, top, offset y −16. It moves in from the bottom with a fade and a spring (0.4, 0.6), and fades out after 1.3s over 0.4s.
- **Mint:** three ✨ sparkles at 22pt (−11, −9), 15pt (13, 5) and 12pt (1, 13) scale in with a spring (0.35, 0.5) and fade out after 0.9s.
- **Dex link:** when the current species is in the dex:
  - Hovering adds a background of primary at 10% opacity with radius 12, animated in 0.12s.
  - The cursor becomes a pointing hand.
  - The tooltip reads "View entry in collection".
  - Clicking opens that dex detail page.

  The egg has no link.

### 5.6 EvoLineView (View:318-550)

- **Nodes** (Store:297-322):
  - Every realized `pathIDs` entry is shown. The one at `stageIndex` is `current` and the rest are `done`.
  - Starting from the current tree node, follow children while each node has exactly one child, adding each as a `future` species.
  - If a node with more than one child is reached, append **one** `mystery` node. Planned branch choices stay hidden until the evolution happens.
- **Layout:**
  - An HStack aligned to the top, with spacing 2.
  - Between nodes: an `arrow.right` symbol at `thumb×0.2` (8pt) in tertiary colour, in a frame `thumb×0.25` (10pt) wide, with `thumb×0.4` (16pt) of top padding.
  - Each node is a column with spacing 1 and width `thumb` (40). When names are shown the width is `thumb+6`.
- **Node content:**
  - Species: SpriteView at `thumb` size, static, using the line's shiny flag and form.
  - Mystery: a "?" at `thumb×0.55` in bold rounded type, in a 40×40 frame, with accessibility label "Unknown next evolution".
  - Future nodes: opacity 0.32 and saturation 0.4.
  - Current node: a 4×4 accent-coloured circle centred at the bottom, offset 2pt down.
- **Overflow:** `rowWidth = n·(thumb+names?6:0) + (n−1)·thumb·0.25 + (2n−2)·2`. When this exceeds `maxWidth` (320):
  - The row scrolls horizontally with no scroll bar.
  - A 24pt edge fade is masked on any side that has more content.
  - Chevron buttons appear on those sides: 16pt circles on regularMaterial with a primary@12% border, holding a 9pt bold chevron. The top padding is `max(0, thumb/2 − 8)`.
  - Each chevron pages by the number of visible columns: `stride = thumb + slack + thumb·0.25 + 4`. Scrolling uses an easeInOut 0.25s animation with leading anchoring.
  - Fade and chevron visibility animate over 0.15s.
- Each species node in the dex is a DexEntryLink with radius 6.

### 5.7 Interactions and states

| State | Shown |
|---|---|
| First run (eggUsage 0) | "Token Egg", "🥚 Incubating", an empty bar, "5M to hatch", the first-run hint and "Hatching soon." |
| Egg ≥ 90% | "About to hatch!" in orange and the wiggle |
| Egg ready but the hatch failed (offline, no candidate, tier gate) | "⏳ Hatching is delayed — retrying on the next refresh" in orange |
| Line not yet loaded after relaunch | The name shows "Token Egg" (`displayName` needs `currentLine`), no evolution line and no "Final form". The sprite and progress still render |
| Details loading or failed | No type badges. The fetch runs again whenever `currentSpeciesID` changes |
| Offline sprites | The static sprite, else the 🥚 glyph. GIF retries continue in the background |

Notifications use the key `companionNotifications` (default true) and are sent only from the bundled `.app`. A notification can have one of these titles, or the candy title:

| Event | Title (en / ko) |
|---|---|
| Hatch | "🥚 Hatched!" / "🥚 부화!" |
| Shiny hatch | "✨ Shiny Pokémon!" / "✨ 이로치 포켓몬!" |
| Evolve | "✨ Evolved!" / "✨ 진화!" |
| Graduate | "🎓 Graduated!" / "🎓 졸업!" |
| Ditto reveal | "🎭 Huh? It's Ditto!" / "🎭 어라? 메타몽!" |
| Shiny Ditto reveal | "🎭✨ Huh? A shiny Ditto!" / "🎭✨ 어라? 이로치 메타몽!" |

Bodies:
- **Shiny hatch:** "A shiny {name} hatched! (1 in {D})"
- **Graduate:** "{name} — saved to your Pokédex! A new egg has arrived."
- **Prayer hatch:** one of five variants chosen by shiny × legendary and common.
- The full set of 8 languages is in Localization.swift:1363-1445.

The floating pet also receives `latestEvent {seq, text: body, at}`. Candy grants are excluded.

---

## 6. Windows adaptation

| macOS mechanism | Windows / Tauri equivalent |
|---|---|
| `~/Library/Application Support/PokeTokenBar/` via `AppStatePaths`, honouring `PTB_STATE_DIR` | `app.path().app_data_dir()`, which is `%APPDATA%\com.tkdals2317.poketokenbar\`. Honour the same `PTB_STATE_DIR` env var. Put `base-index.json`, `sprites\`, `pokemon-details-v1\` and `pokemon-names-v1\` there too. Keep them in the state dir rather than copying the macOS quirk of ignoring the override |
| `JSONEncoder` + `Data.write(.atomic)` | `serde_json` + temp file + `fs::rename`, as in the existing `save_json`. On Windows, `rename` over an existing file works with std's `MoveFileExW(REPLACE_EXISTING)` |
| `UserDefaults` (`growthDifficulty`, `shopDifficulty`, `alwaysFacesBack`, `companionNotifications`) | `settings.json` (`settings.rs`). Move `growth_difficulty` out of the companion state into Settings to match: a save import must not change the difficulty |
| `UNUserNotificationCenter` gated by `AppEnv.isBundledApp` | `tauri-plugin-notification` (already used). Gate on `!cfg!(debug_assertions)` or an installed-build flag. Windows toasts need the installed AUMID; dev builds have none, as on macOS |
| `NSStatusItem` title "1.2M / Growth 42%" (`menuGrowthText`) | The tray cannot show text on Windows. Put `menuGrowthText` in the tray tooltip (`set_tooltip`) and in the pet's hover title. `set_title` can stay for macOS |
| Status item icon showing the representative sprite | `TrayIcon::set_icon` with a 16/32px PNG made from the cached static sprite (`image` crate, nearest-neighbour downscale). Shiny and back variants follow `RepresentativeSubject` |
| Floating `NSPanel` pet (back view, event bubble, prayers) | The existing `pet` webview window. Feed it `representativeSubject`, `facesBack` and `latestEvent` through the `companion` event payload |
| `URLSession` | `ureq` (already used) with a 15s timeout. GraphQL POST with `Content-Type: application/json` |
| `NSImage`/`CGImageSource` GIF decode, `NSCache`, frame-rate cap | WebView2 `<img>` plays GIFs natively. For cache plus offline support, serve the cached files from Rust: download into `sprites\` and expose them with `convertFileSrc` (asset protocol scope = app data `sprites/**`) or a custom `sprite://` protocol. The always-visible pet's frame-rate cap needs decoding (`gifuct-js` in TS, or the `gif` crate in Rust plus canvas playback). Without decoding, the pet runs at native 20fps; see the open questions |
| Egg content crop (`cropToContent`) | CSS `object-fit` cannot crop transparent padding. Pre-crop `egg.png` in Rust (`image` crate) once and cache it as `egg-cropped.png`, or hard-code the crop: the egg content is 28×30 inside 96×96, centred |
| `NSCursor.pointingHand`, `.help()` tooltips | CSS `cursor: pointer` and the `title` attribute |
| SF Symbols `arrow.right`, `chevron.left/right` | Inline SVG or the Unicode → ‹ › characters, with matching sizes |
| `.regularMaterial` | `background: color-mix(in srgb, var(--card) 80%, transparent); backdrop-filter: blur(12px)` |
| SwiftUI spring animations | CSS transitions or `@keyframes` with `cubic-bezier`. Spring (response 0.3, damping 0.55) ≈ `cubic-bezier(.34,1.56,.64,1)` over 300ms; spring (0.5, 0.55) ≈ the same curve over 500ms |
| `Locale.preferredLanguages` | `sys_locale` / `GetUserDefaultLocaleName`. The existing `system_language()` is fine; extend it to 8 languages if the app grows to 8 |
| `localizedCompare`, diacritic-insensitive search | `Intl.Collator(lang, {sensitivity:'base'})` (dex area) |
| `SystemRandomNumberGenerator` | `fastrand::Rng::new()` (already used). Use `u64(..)` for every `rng.next()` and keep the `% n` semantics |
| Keychain, LaunchAgent, URL schemes | Not used by the companion area |

---

## 7. Gaps vs the current Windows code

### 7.1 Rules and balance (`companion.rs`)

1. **No repeat-growth boost.** `Mon` has no `has_growth_boost`, and `phase_threshold` has no multiplier parameter. Add the ×2 halving with Swift's double rounding.
2. **`scaled()` adds `.max(1)`**, which Swift does not. This is harmless but does not match.
3. **`clamp_difficulty` maps `d <= 0` to 1.0.** Swift clamps it to 0.1. Only non-finite values become 1.0.
4. **The difficulty is stored in the save** (`State.growth_difficulty`; 0 means 1). Swift keeps it in user preferences. There is also no shop difficulty and no slider log mapping or snap. Settings.tsx must use the `position↔value` mapping and the ±1% default snap.
5. **The difficulty rescale is wrong.** `set_difficulty` uses `round(v·new/old)`. Swift uses floor with rounded thresholds and never lets an incomplete stage become complete (`min(newThr−1, v)`). The current code can complete a stage by rounding up, and the next tick then evolves it. The launch migration of the old 0.0001–20 range does not exist.
6. **The ledger runs without data.** `credit()` seeds the baseline on the first call even when nothing has been read. `refresh()` always passes `{claude, codex}`, including zeros. Swift requires `hasUsageData && !map.isEmpty` and only passes providers with today-dated data.
7. **New-day ledger.** The Windows code replaces the ledger with today's map. Swift keeps every known provider at 0 so that a provider reporting late is credited in full. As written, a provider absent from the first refresh of a day loses its earlier-day usage.
8. **`credit` returns the delta even before the baseline is set.** It returns 0 there, which is correct, but `feed()` is also called unconditionally. With Swift's guard nothing is fed before the baseline. The current behaviour only matches by accident.
9. **No regression log or legacy-ledger (`nil`) seeding path.** This is minor.
10. **No egg tier, pending species, pre-roll or prefetch.** `hatch_if_ready` rolls and fetches inside one tick. There is no `isHatchRetryDelayed`, so the UI cannot show the delayed state.
11. **No REST fallback** when GraphQL is down and there is no cache. The hatch blocks until GraphQL comes back. There is also no background REST index build.
12. **No shiny charm odds** (48). `SHINY_DENOMINATOR` is hard-coded to 64.
13. **No Ditto disguise, reveal or profile rebase.** `DITTO_ID` is only excluded from the index.
14. **No forms at all:** no catalog, no line forms, no roll weighting, no Unown 28 letters, no form-aware sprites or names, and no form-based ownership.
15. **No individual profile:** no IVs, gender, ability, moves, level, stats or Shedinja rule, and no `PokemonDetails` fetch or cache.
16. **No profile migration**, and therefore no `.pre-profiles-v1` backup.
17. **No release record.** There is no egg purchase yet (shop area), but when it lands it must use Swift's reached-prefix rule.
18. **Graduation `DexEntry` is missing fields:** `id`, `form`, `profile`, `prayers`, `namesVersion` and `releasedAt`. `caught_at` is RFC 3339 rather than a Swift Date. Field names are snake_case (`chain` vs `chainOrder`, and so on).
19. **No `eventUntil` or levelUp window**, no `justEvolvedTo`/`justGraduated`, and no display state (idle, working, focus, tired, sleep, sulk) at all. There is no burn tier, limit warning or idle tracking.
20. **No back view or `alwaysFacesBack`.**
21. **Evolution planning is close but not identical.**
    - On a stale plan, Windows truncates `planned_ids` to `stage_index` and appends a route from `node`. Because `route[0] == node.id == current`, the plan ends up correct only by coincidence.
    - Windows does not repair `totalForms` and has no `normalizedEvolutionState`. Saves whose line changes upstream (for example the sprite bound) are not normalized on load.
    - There is no 50-iteration guard.
22. **`Mon` embeds the whole `Line`,** including the tree and every name, in the save. Swift stores only IDs and refetches the line; names are cached. This is a schema divergence; it is fine as long as the import path converts.
23. **No sanitize-on-load:** no token clamps, `totalForms ≤ 12`, deduplication or `stage_index` clamp. `current_id()` panics if `path_ids` is empty (Swift falls back to `baseID`). `load_json` resets everything when any field is malformed. Swift decodes field by field and isolates each dex entry.
24. **No snapshot recovery when the file is corrupt.** It is renamed to `.json.corrupt` and the app starts fresh.
25. **No representative reconciliation in Rust.** `settings.representative` lives in Settings, is species-only (no form) and is reconciled only in TS (`displayed()`). Shiny is "owned any shiny of the species", which is not form-aware and does not apply the Ditto `displaysShiny` rule.
26. **`ownedSpecies` in TS** correctly uses only the reached stages (`path_ids.slice(0, stage_index+1)`). But it reads the raw `is_shiny`, so once the Ditto disguise exists it would reveal a disguised Ditto's shiny early. It must use the `displaysShiny` rule.
27. **`menuGrowthText` is missing.** The tray tooltip shows only today's tokens and the limit.
28. **The overflow-before-celebration ordering, the generation guards against save import, and the 50-iteration loop guard** are all missing. The import guards matter once save transfer exists.

### 7.2 PokéAPI (`pokeapi.rs`)

29. **Names are filtered to `ko, en, ja-hrkt, ja`.** Swift keeps every language, with codes normalized (lowercase, `_` → `-`, trimmed). Adding a language later needs a refetch.
30. **The chain URL check is `starts_with("https://pokeapi.co/api/v2")`.** Swift checks scheme `https` and host `pokeapi.co`. Both are fine, but the `line(base)` subtree is cut with `t.find(base_id)`, whereas Swift uses the whole chain. These are equivalent for base species. Keep it, but for Ditto `line(132)` it works as is.
31. **`base-index.json` format is `[[id, rate], …]` and freshness comes from the file mtime.** Swift writes `{fetchedAt, entries:[{id,captureRate}]}`.
32. **No line cache in memory or on disk.** `line()` refetches every species on every hatch (5–9 requests). Swift caches lines per session.
33. **No `pokemon/{id}` details, no `pokemon-names-v1` resources** (type, ability, move, pokemon-form), no `form_names` handling, and no request deduplication.
34. **GraphQL is not checked for a non-200 status.** `ureq` errors on 4xx and 5xx anyway. This is fine.

### 7.3 Sprites (`shared.ts`)

35. Only the animated GIF URL is built, straight from GitHub raw on every render. There is **no disk cache**, no offline display, no static PNG fallback (species above 649, or a GIF failing to load), no shiny-to-plain fallback, no back sprites, no form asset names (`201-b`, `10006`), no retry backoff and no egg crop.
36. The `<img>` uses `height: 96px; image-rendering: pixelated`. Swift uses a 76pt tile with aspect-fit, a background and rounded corners.

### 7.4 Companion card (`App.tsx` `CompanionCard`)

37. **Layout differs.** Swift uses a left sprite tile with a right-hand info column. Windows uses a centred column with a 20px `h2`.
38. **The egg title** is the localized "알"/"Egg" instead of "Token Egg". There are no incubating or imminent text, wiggle, guarantee badge, delayed message or first-run hint.
39. **No status line,** no type badges, no default-form note, no growth-boost badge and no "Final form" / "Stage i / k" text. Nature is shown as "{nature} 성격" next to the rarity.
40. **The progress bar** is an 8px green `--accent` bar with a "label · X / Y" line underneath. Swift uses a 6→12px orange capsule with hover detail "X / Y · Z%" inside it, then the "N to next evolution" / "N to graduation" / "N to hatch" texts. `fmt` uses `Intl` compact notation, not `TokenFormatter.compact` (for example "1.9B" vs "1.88B").
41. **The evolution line shows `planned_ids` with every future stage as `?`.** Swift shows future single-path species dimmed (opacity 0.32, saturation 0.4) and one `?` only at a branch. Windows has no arrows, current-stage dot, horizontal overflow handling, dex links or name labels. `isFinal` uses `stage_index === planned_ids.length - 1` instead of the tree-leaf test.
42. **Rarity badge colours** are GitHub palette values (#6e7781, #1f883d, #0969da, #bf8700) with a 6px radius and 12px text. Swift uses system gray, green, blue and orange in an 8pt bold uppercase capsule.
43. **No celebrations** (flash, pop, ✨, 🎭, candy XP, mint sparkles). Windows only shows a 6-second toast with its own strings.
44. **Notification text differs.** Windows uses settings strings keyed hatched, shiny, evolved and graduated, with the species name as the body. Swift has separate titles and bodies, the evolve body "{old} evolved into {new}!", the shiny odds "(1 in D)", prayer variants and the Ditto reveal.
45. **Languages:** Windows has 3 (ko, en, ja), Swift has 8. The 25 nature names in es, fr, pt, de and ru are not ported.

---

## 8. Open questions

1. **Save compatibility.** Should Windows read and write the macOS `companion-state.json` schema (camelCase, Swift reference-date `Double` dates, string-keyed int maps) so saves can move between platforms through SaveTransfer? If so, the Rust structs need `#[serde(rename_all = "camelCase")]`, a Date adapter and a one-time migration from the current `companion.json`. If not, decide whether the Windows schema should still add every Swift field so features can be ported one by one.
2. **Provider set.** Swift credits up to 16 providers keyed by `UsageProvider.id` (for example `claude`, `codex`, `gemini` …). Windows reads only `claude` and `codex`. Ledger keys must use the same IDs as macOS if saves are to be portable.
3. **`hasUsageData` on Windows.** It needs a definition, for example "at least one provider produced a today-dated snapshot". Without it, the baseline and ledger gating from Swift cannot be reproduced.
4. **Frame-rate cap for the pet window.** Is native 20fps GIF playback in WebView2 acceptable on battery, or should the pet decode and decimate frames like `capFrameRate`? The macOS default floor is 0.4s, the `powerSaver` preset.
5. **The Ditto disguise in dev builds.** Swift turns it off outside the bundled app (`AppEnv.isBundledApp`). Should Windows gate it on release builds as well?
6. **The 30-day details and names caches.** They live in the state dir in Swift, while base-index and sprites ignore `PTB_STATE_DIR`. The recommendation is to put everything under app data on Windows. Confirm that the macOS quirk does not need to be reproduced.
7. **Where the egg prayers UI goes.** Swift only lets the user pray from the floating pet (a click on the egg), and prayers affect only notification text and the dex detail page. Confirm whether that belongs to the pet parity area or this one.
8. **Language count.** Swift has 8 UI languages. Expanding Windows beyond ko, en and ja affects the names storage (keep all languages: gap 29) and the nature tables.
