# Parity spec: floating pet, settings, app services

Source of truth: PokeTokenBar v2.5.7 (Swift/macOS). Paths below are relative to `Sources/PokeTokenBar/`.
Target: this repo (Tauri 2, `src-tauri/src` + `src`).

---

## 1. Overview

This area covers everything outside the popover's usage/limits/dex views:

- **Floating pet** (`UI/FloatingPetPanel.swift`): a borderless always-on-top window that shows the
  representative Pokémon (or egg). It handles drag, click (pray over an egg or open the popover), right-click
  menu, a hover callout with today's usage, limit-alert and agent speech bubbles, a pixel heart after stroking,
  🙏 particles and a wobble on prayer, a back sprite after 3 h idle, an event callout that pops up by itself, and
  "run to the cursor" when an agent reports `done`.
- **Agent hooks** (`Core/AgentActivity.swift`): the `poketokenbar://agent?...` URL scheme. It is a state machine
  with one entry per label, for up to 8 agents.
- **Settings** (`UI/SettingsView.swift`, `UI/DifficultySettingsSection.swift`): 10 grouped sections, shown in
  the 360 pt popover at 460 pt height.
- **Save transfer and snapshots** (`Core/SaveTransfer.swift`, `Core/SaveSnapshots.swift`): an envelope-format
  export/import with backups (5 kept), and local auto/manual snapshots (10 kept, auto every 12 h).
- **App services**: `UpdateChecker`, `Localization` (8 languages), `CrashReporter`, `AppLog`, `SingleInstance`,
  `LoginItem`, `AppStatePaths`.

---

## 2. Features (numbered)

### Floating pet
1. Opt-in overlay (`floatingPetEnabled`, default **off**). It shows the representative subject's sprite at
   `floatingPetSize` pt (default 96, range 48–384, step 8).
2. Drag anywhere on the sprite to move it. The pet origin (not the panel origin) persists. It cannot activate
   the app or take focus.
3. Click (movement² < 16 pt², about 4 pt):
   - if the subject is an **egg**, **pray**: `eggPrayers.count += 1`, spawn a 🙏 particle, wobble the egg, and
     refresh usage at 30-prayer milestones (throttled to 30 s);
   - if the subject is a **hatched Pokémon**, open the popover.
   - In both cases the click first dismisses every waiting/done agent bubble.
4. Right-click (or control-click) opens a context menu with three items: **Open Token Bar** (always opens the
   popover, even for an egg), **Turn off floating pet** (sets `floatingPetEnabled=false`), and **Show back view**
   (checkmark toggle of `alwaysFacesBack`).
5. The hover callout is a rounded label above the pet: "Today: N tokens [(limit P%)] [· Sulking for H hours |
   D days]", then an optional event line, then an optional prayer line. While the callout is up, the native
   tooltip carries the same text.
6. Event pop: a new hatch/evolve/graduate/Ditto-reveal line pops the callout by itself for **6 s**. It is not
   queued: an event that cannot pop now is dropped.
7. Limit-alert speech bubble (when `floatingPetBubbleAlerts` is on, the default) shows for **6 s**. Critical
   titles are red.
8. Stroking: 3 horizontal direction reversals inside 1.2 s, each travelling at least `max(10, size*0.15)` pt,
   show a pixel-art heart bubble for **1.8 s**. Every stroke restarts the timer.
9. Sulk / back view: after **3 h** without token usage the companion shows its **back sprite** on every
   surface, and the hover appends the sulk duration. The `alwaysFacesBack` setting forces the back view. An egg
   never shows a back.
10. Agent reactions (`floatingPetAgentActivity`, default on):
    - **working**: two-frame squash "bob" every 0.5 s;
    - **waiting**: speech bubble that stays until the next event or a click;
    - **done**: bubble for 6 s, and the pet animates over 0.6 s to sit beside the cursor, then returns home.
11. Bubble priority: limit alert > agent done > agent waiting > hearts > hover/event callout.
12. Energy and accessibility: Low Power Mode rebuilds the view non-animated. Reduce Motion or Low Power
    disables bob and run. The view tree is released while the pet is hidden or the display is asleep.
13. The hidden pet always comes back at home, never at a run position.

### Agent URL scheme
14. Format: `poketokenbar://agent?state=<working|waiting|done|idle>[&message=<text>][&label=<name>]`.
    Unknown scheme, host or state is ignored.
15. Up to 8 labelled agents are tracked. `working` times out after 10 min, `done` after 6 s, and `waiting`
    never expires.
16. The bubble title is "<label> · waiting +N" or "<label> · done", or the plain title when there is no label.
    It is fitted to one line, with the body fitted to two lines.

### Settings (10 groups, in order)
17. General: Language, Representative Pokémon, Refresh interval, Animation, Limit display, Daily trend period,
    [Tracked Claude account when there is more than one account], Launch at login.
18. Difficulty: Growth and Shop prices log sliders (10 %–200 %), with an explicit **Save** button.
19. Show in menu bar: Today's tokens, Today's cost ($), Pokémon growth %, Limit % [+ Percent color], and a footer
    hint.
20. Tracked providers: one toggle per provider (16 providers), with a footer hint.
21. Floating Pet: Show floating pet, Show back view, [Size, Show notifications as bubbles, React to agent
    activity].
22. Notifications: Limit alerts [+ Warning/Critical sliders], Companion events, Provider status checks.
23. Updates: Update notifications, Check for updates [+ result row].
24. Backup & Transfer: Export save, Import save, Create snapshot, collapsible snapshot history with Restore.
25. Advanced (collapsed by default): session key(s), Disable Keychain, Refresh limit token, Additional Claude
    accounts, Additional scan folders, aggregation note.
26. About & Support: Report a problem (mailto), Show log file (Finder), and a fallback address on failure.
27. Footer: `v<version> · GitHub · Website · ♥ Sponsor`.

### Services
28. Update check against the GitHub latest release, with skip-version and a Homebrew in-place upgrade.
29. Crash reporting: unclean-exit marker, signal handlers, and the crash log merged into the main log on the
    next launch.
30. A rotating app log (2 MB, one `.old` generation).
31. Single instance: the later-started instance yields.
32. Launch at login through a KeepAlive LaunchAgent, which also restarts the app after a crash.
33. A state directory override through the `PTB_STATE_DIR` environment variable.

---

## 3. Rules & constants

### 3.1 Floating pet controller (`FloatingPetPanel.swift`)

| Constant | Value | Cite |
|---|---|---|
| Persisted origin keys | `floatingPetOriginX`, `floatingPetOriginY` (Double, **pet** bottom-left in screen pts, AppKit y-up) | :32-33 |
| `clickThresholdSquared` | 16 (≈4 pt) | :36 |
| `bubbleHeadroom` | 72 pt above sprite while any bubble/hearts shown | :40 |
| `bubbleMinWidth` | 180 pt (panel width while bubble shows = `max(petSize, 180)`) | :42 |
| `bubbleHorizontalPadding` | 8 pt each side | :44 |
| `bubbleContentWidth` | 180 − 16 = **164 pt** | :46 |
| `bubbleBodyLineLimit` | 2 | :49 |
| `agentRunDuration` | 0.6 s, ease-in-ease-out | :52, :493 |
| `agentRunCursorGap` | 16 pt | :54 |
| `agentBobInterval` | 0.5 s (half a cycle; sleep tolerance 10 %) | :56, :1205 |
| `heartDuration` | 1800 ms | :187 |
| `eventPopDuration` | 6 s | :268 |
| `prayersPerRefresh` | 30 (= `L.prayersPerQuip`) | :266 |
| `prayerRefreshInterval` | 30 s | :279 |
| Stroke min distance | `max(10, petSize * 0.15)` pt (screen x) | :632 |
| `PettingDetector.requiredReversals` | 3 | :1256 |
| `PettingDetector.window` | 1.2 s | :1257 |
| `PrayerBurst.lifetime` | 1100 ms (float-and-fade is 1 s easeOut) | :1115, :1184 |
| Prayer emoji size | `clamp(petSize*0.25, 14, 32)`; glyph font = emojiSize*0.8 | :1140, :1168 |
| Default pet origin | `(visible.maxX − petSize − 24, visible.minY + 24)` of main screen (bottom-right); fallback `(120,120)` | :793-796 |
| Hover callout offset | centred on pet panel `midX`, bottom edge 6 pt above panel top | :748 |
| Tooltip delay | `NSInitialToolTipDelay` = 600 ms (registration default) | PokeTokenBarApp.swift:101 |
| Limit bubble TTL | 6 s (`UsageStore.shouldDismissBubble ttl: 6`) | UsageStore.swift:2161, :2269 |
| Sulk threshold | `PokemonBalance.sulkAfterIdle = 3 * 3600` | CompanionModel.swift:109 |

**Panel geometry** (`panelSize`, `panelOrigin`, `petOrigin` :516-532):
```
showingBubble ? (max(petSize,180), petSize+72) : (petSize, petSize)
xInset = max(0, (panel.w − petSize)/2)
panelOrigin = (pet.x − xInset, pet.y)   // bottom-aligned: the panel grows upward
```
If no screen's `visibleFrame` intersects the computed frame, the code falls back to the default origin (:786-789).

**Move / persist** (`resolveMove` :67, `panelDidMove` :824):
- A user drag at any phase sets the phase to `.home`, persists the new origin, and cancels any run/return by
  bumping `runGeneration` and running a 0 s animation to the current frame.
- A non-drag move persists only when the phase is `.home`. Run animation frames never overwrite home.

**Run to cursor** (`runPanelOrigin` :75):
```
fitsRight = cursor.x + gap + w <= vf.maxX ; fitsBelow = cursor.y − gap − h >= vf.minY
x = fitsRight ? cursor.x+gap : cursor.x−gap−w
y = fitsBelow ? cursor.y−gap−h : cursor.y+gap     // AppKit y-up: "below" = smaller y
clamp x to [vf.minX, vf.maxX−w], y to [vf.minY, vf.maxY−h]   (min first then max → bottom-left wins when larger than screen)
```
The screen is the one that contains the cursor, falling back to the main screen. The run always uses the bubble
size.
- `runsToCursor = phase != .away && motionAllowed && panel.isVisible` (:89). A second `done` while the pet is
  away does not run again.
- `returnHome` (:468): when the phase is not away or the panel is hidden, set home and sync. When motion is not
  allowed, jump home. Otherwise set `.returning`, animate to the home frame, and on completion set `.home`, but
  only if the run generation still matches.
- `motionAllowed = !lowPower && !reduceMotion` (:86, :378). A test override is available.

**Bubble selection** (`bubble(limitAlert:agent:)` :105):
`limit` → `agentDone(label,message)` → `agentWaiting(label,message,others)` → nil.
Hearts show only when there is no limit alert and no agent bubble (:190). `isShowingBubble = alert ||
agentBubble || hearts` (:196). The native tooltip is nil while hearts show (:203).

**Bubble content** (:120-184):
- limit: title is `notifCritical` ("Limit imminent") when critical, otherwise `notifWarning` ("Limit warning").
  The body is `alert.body(l)` = "<name> at <pct>[ · <account>]". The account is dropped when the body would wrap
  past 2 lines.
- waiting: `agentTitle(label, unlabeled: "Your agent needs you", status: "waiting", count: others>0 ? "+N" :
  nil)`. The body is the message, or "It's waiting for your input."
- done: `agentTitle(label, unlabeled: "Done!", status: "done", nil)`. The body is the message, or "Your agent
  finished its task."
- `agentTitle`: the title is `lead + suffix`, where `suffix = label==nil ? "" : " · "+status`, plus `" "+count`.
  If its single-line width (11 pt bold) exceeds 164 pt, binary-search the longest prefix of `lead` such that
  `prefix.trim + "…" + suffix` fits. Then, if the prefix has a space at index ≥ low/2, cut at that space.
- `fittedBubbleBody`: binary-search the longest prefix such that `prefix.trim + "…"` wraps to ≤ 2 lines at
  164 pt, 10 pt regular.
- Measurement: title is system 11 pt bold, body is system 10 pt. Bubble height = title.h + 2 + body.h + 12
  (vertical padding) + 6 (tail).

**Click routing** (`PetClickRouter.handle` :872):
```
egg:  pray(); spawnEmoji(); didPray()  (refresh hover text)
      count = eggPrayers.count; milestone = count/30
      if milestone < lastRefreshedMilestone → lastRefreshedMilestone = milestone   (new egg)
      if milestone > lastRefreshedMilestone && (lastRefreshAt==nil || now−lastRefreshAt ≥ 30s):
          lastRefreshedMilestone = milestone; lastRefreshAt = now; store.refresh()
pokemon: openPopover()
```
`pray()` is a no-op when there is an active Pokémon. It increments and persists `state.eggPrayers.count`
(CompanionStore.swift:1406). Router state lives in memory only.

**Event pop** (:660): only when `event.seq != lastSeenSeq && panelVisible && !showingBubble`. `lastSeenSeq` is
updated either way. At init `lastSeenEventSeq = companion.latestEvent?.seq`, so events from before the pet
existed are not news. Events come from `notifyCompanionEvent(showsOnPet:true)` (CompanionStore.swift:1543): the
hatch, evolve, graduate and Ditto bodies. Candy grants use `showsOnPet:false`. The event text is the
notification **body** (e.g. "Pikachu hatched from the egg!").

**Hover text** (`FloatingPetView.hoverTooltip` :1086):
```
usage = grouped(todayTotalTokens)      // locale decimal grouping, e.g. "12,400,000"
base = "Today: \(usage) tokens"
if highestLimitUtilization != nil:
   pct = percent(displayPercent(util, mode)); base = "Today: \(usage) tokens (limit \(mode==.remaining ? "\(pct) left" : pct))"
if sulkIdle > 0: h = Int(sulkIdle/3600); base += " · " + (h < 48 ? "Sulking for \(h) hours" : "Sulking for \(h/24) days")
if eventLine: base += "\n" + eventLine
if prayers > 0 (only when subject is egg): base += "\n" + "\(n) prayers · <quip>"   ("1 prayer" singular)
```
Quips: index = `min(n/30, 10)`, 11 quips (Localization.swift:1373-1402).
`sulkIdle = displayState == .sulk ? idle : 0`.

**Sulk / back view** (CompanionStore.swift:18, :1987-1993):
`facesBack = state.active != nil && (alwaysFacesBack || displayState == .sulk)`.
The `.sulk` state comes from `computeState`, in this priority order: no active Pokémon → egg; graduation/event
window → levelUp; limit warning → tired; `hasUsageData && idle >= 3h` → sulk; no data or today==0 → sleep;
otherwise the burn tier. `effectiveIdle = min(reportedIdle, now − lastObservedRiseOfTodayTotal)` (:1974). A new
day or a first sighting only sets the baseline, and unknown idle never sulks. A missing back sprite falls back
to the front sprite.

**Stroke detector** (`PettingDetector.feed` :1269). Port it verbatim:
```
if last==nil || t−last > 1.2: reset; last=t; extreme=x; return false
last=t; drop reversals older than 1.2 s
if dir==0: if |x−extreme| ≥ min: dir = sign(x−extreme); extreme=x; return false
if (x−extreme)*dir > 0: extreme=x; return false          // still same way
if (extreme−x)*dir ≥ min: dir=−dir; extreme=x; reversals.append(t); return reversals.count ≥ 3
return false
```
It is fed only by hover moves with **no button pressed**, using screen x. The detector resets on mouseDown,
mouseDragged and mouseExited.

**Prayer wobble keyframes** (:1049): rotation −9° (0.06 s) → +9° (0.10 s) → −5° (0.08 s) → 0° (0.06 s). It runs
once per prayer. The 🙏 particle starts at y = petSize*0.7 and ends at emoji/2 (top), with
x = emoji/2 + xFrac*(petSize−emoji), `xFrac = random 0…1`, and opacity 1→0.

**Agent bob** (:1193): `scaleEffect(x: squashed ? 1.03 : 1, y: squashed ? 0.94 : 1, anchor: .bottom)`. It
toggles every 0.5 s with no interpolation.

**Bubble/hearts transitions**: spring(response 0.3, damping 0.7). The speech bubble inserts with scale 0.8 from
the bottom plus opacity. Hearts insert the same way, and their removal is instant.

### 3.2 Agent activity (`Core/AgentActivity.swift`)

| Constant | Value | Cite |
|---|---|---|
| scheme / host | `poketokenbar` / `agent` (case-insensitive) | :14-15 |
| `maxMessageLength` | 80 chars (scalars cap 320) | :18-21 |
| `maxLabelLength` | 24 chars (scalars cap 96) | :23 |
| `workingTimeout` | 600 s | :97 |
| `doneDuration` | 6 s | :98 |
| `maxAgents` | 8 | :100 |

- Parsing (:30): `state` is required and lowercased. `message` and `label` are optional, using the first query
  item with that name. `idle`/`working` ignore the message.
- Sanitize (:54): Unicode category Cc becomes a space. Category Cf is dropped, except U+200D. Whitespace runs
  collapse to a single space and the text is trimmed (split on whitespace, joined with " "). Empty → nil. When
  scalars exceed 4×max, cut to 4×max−1 scalars, trim, and append "…". When the character count exceeds max, cut
  to max−1 characters, trim, and append "…".
- Tracker (:149):
  - Key = label, or `""` when there is none.
  - `apply` first expires stale entries.
  - `idle` removes the key.
  - `working` sets a deadline of now+600, `done` now+6, and `waiting` no deadline.
  - A new key when 8 are already stored evicts the entry with the smallest sequence.
  - The sequence is incremented on every apply.
- Summary (:117): `working = any working`. `done` = the done entry with the highest sequence. `waiting` = the
  waiting entry with the highest sequence, and `otherWaitingCount = waiting.count − 1`.
- `dismissBubble` (click) keeps only `.working` entries (:179).
- One timer is armed at `min(deadlines)`. An early fire re-arms (:427).
- Gate: events are dropped when `!(floatingPetEnabled && floatingPetAgentActivity)` (:385, :393). Turning either
  setting off resets the tracker, the timer and the run phase (:593).
- After `agentDidChange`: when `summary.done != nil` and run is allowed, run to the cursor; otherwise sync. When
  there is no done, `returnHome()`.
- macOS registers a raw Apple Event `kAEGetURL` handler in `applicationWillFinishLaunching`, so the URL never
  activates the app or opens a window (PokeTokenBarApp.swift:63-73). Events that arrive before launch finishes
  are dropped.

### 3.3 Settings: every row

Rows are listed in render order. *Cond.* means the row appears only under that condition.

| # | Group | Row (en label / hint) | Control | Range / options | Default | Persist key |
|---|---|---|---|---|---|---|
| 1 | General | Language | menu picker | 한국어, English, 日本語, Español, Français, Português, Deutsch, Русский | system (`preferredLanguages[0]` prefix ko/ja/es/fr/pt/de/ru, else en) | `CompanionState.language` (companion-state.json) |
| 2 | General | Representative Pokémon | menu (150 pt wide, trailing, small) | "Follow current companion" (✓ when nil) / "Choose in Pokédex…" → navigates to dex | nil → label "Follow current companion"; else `#<id> <name>[ ✨]` | `CompanionState.representativeSpeciesID/Form` |
| 3 | General | Refresh interval | menu | Manual(0), 1 min(60), 2 min(120), 5 min(300), 15 min(900) | **120 s** | `refreshInterval` (seconds) |
| 4 | General | Animation / "Smoother uses more battery" | menu | Power saver (0.4 s floor ≈2.5 fps), Balanced (0.2 s ≈5 fps), Smooth (0.1 s ≈10 fps). Low Power forces ≥0.4 | `powerSaver` | `animationQuality` (`powerSaver|balanced|smooth`) |
| 5 | General | Limit display | **segmented** | Used / Remaining | `used` | `limitDisplayMode` |
| 6 | General | Daily trend period | menu | Last 7 days(`7d`), Last 14 days(`14d`), Last 21 days(`21d`), This month(`month`) | `14d` | `dailyTrendWindow` |
| 7 | General *(cond. claudeAccounts > 1)* | Tracked Claude account / "Drives the menu bar limit, warning state, companion mood and 5h forecast." | stacked menu below the label | Automatic (last used), Highest usage, then each account title | automatic | `ClaudeTrackedAccountMode.defaultsKey` |
| 8 | General | Launch at login (+ "Available only when installed as an .app bundle…" when not bundled, + red error) | switch (small) | on/off; disabled when not bundled | system state | LaunchAgent registration (not stored) |
| 9 | Difficulty | Growth | log slider + value | Easy ← → Hard; 10 %–200 % | 100 % | `growthDifficulty` (Double 0.1–2.0) |
| 10 | Difficulty | Shop prices | log slider + value | Cheap ← → Expensive; 10 %–200 % | 100 % | `shopDifficulty` |
| — | Difficulty | **Save** (borderedProminent, right-aligned) | button | shown only when draft ≠ stored | — | — |
| 11 | Show in menu bar | Today's tokens | switch | | **true** | `showTokensInMenu` |
| 12 | Show in menu bar | Today's cost ($) | switch | | false | `showCostInMenu` |
| 13 | Show in menu bar | Pokémon growth % / "Progress toward hatching or your companion's next evolution or graduation" | switch | | false | `showGrowthInMenu` |
| 14 | Show in menu bar | Limit % | switch | | false | `showLimitInMenu` |
| 15 | Show in menu bar *(cond. Limit % on)* | Percent color / "Same colors as the popover limit gauges" | menu (default style) | Match gauges(`gauge`), attention(`attention`), off(`off`) | `gauge` | `menuLimitColorMode` |
| — | Show in menu bar footer | "All off shows only the character" | caption2 tertiary, leading 4 | | | |
| 16 | Tracked providers | one switch per provider, in this order: Claude Code(`claude_code`), Codex(`codex`), Gemini(`gemini`), Antigravity(`antigravity`), OpenCode(`opencode`), Hermes Agent(`hermes`), Cursor(`cursor`), Grok(`grok`), Copilot(`copilot`), Kiro(`kiro`), Pi(`pi`), omp(`omp`), omo(`omo`), Aside(`aside`), Kimi Code(`kimi_code`), Devin(`devin`) | switch | | all on | `disabledProviderIDs` (sorted string array; opt-out list) |
| — | Tracked providers footer | "Turned-off providers are not read. They are excluded from tabs, totals, Pokémon growth, and limits." | | | | |
| 17 | Floating Pet | Show floating pet / "Your Pokémon floats over the screen — drag to reposition" | switch | | **false** | `floatingPetEnabled` |
| 18 | Floating Pet | Show back view / "Always shows its back in the menu bar, floating pet and popover" | switch | | false | `alwaysFacesBack` |
| 19 | Floating Pet *(cond. pet on)* | Size (callout font) | slider + "Npx" (44 pt, monospaced) | 48…384 step 8 | **96** | `floatingPetSize` |
| 20 | Floating Pet *(cond.)* | Show notifications as bubbles | switch | | true | `floatingPetBubbleAlerts` |
| 21 | Floating Pet *(cond.)* | React to agent activity / "Coding agents trigger it with poketokenbar:// links — see the README" | switch | | true | `floatingPetAgentActivity` |
| 22 | Notifications | Limit alerts | switch | | true | `limitNotifications` |
| 23 | Notifications *(cond.)* | Warning (callout) | slider + "%" (38 pt) | 50…95 step 5 | 80 | `warnThreshold` |
| 24 | Notifications *(cond.)* | Critical | slider + "%" | 80…100 step 5 | 95 | `critThreshold` |
| 25 | Notifications | Companion events (hatch / evolve / graduate) | switch | | true | `companionNotifications` |
| 26 | Notifications | Provider status checks / "Show Claude / OpenAI incidents in the popover (not a notification)" | switch | | true | `statusChecksEnabled` |
| 27 | Updates | Update notifications | switch | | true | `updateNotificationsEnabled` |
| 28 | Updates | Check for updates | button "Check now" → spinner while checking | | | |
| 29 | Updates *(cond. checked)* | result: orange "Version X is available" + [Update]; or orange "You skipped vX" + [Show again][Update]; or secondary "You're on the latest (vCUR)" | | | | `skippedUpdateVersion` |
| 30 | Backup & Transfer | Export save / "Saves your Pokédex, lifetime tokens, Bag, and current Pokémon as one file" | button "Export…" | | | |
| 31 | Backup & Transfer | Import save / "Pick a file exported from another Mac and continue here" | button "Import…" | | | |
| 32 | Backup & Transfer | (hint as label) "Saves an instant local restore point of your progress (keeps up to 10)" | button "Create snapshot" | | | |
| 33 | Backup & Transfer | "Automatic Backups (Snapshots)" disclosure (chevron.right/down, secondaryLabel tint, 20 pt tall, 12/8 padding) | toggle; collapsed every time Settings appears | | collapsed | — |
| 34 | *(expanded)* | Empty: "No snapshots saved yet". Otherwise one row per snapshot (newest first): 28 pt sprite (egg when there is no active Pokémon), date (medium date + short time, app locale, caption), "Pokédex N · X lifetime" (caption2 secondary), button "Restore" (small) | | | | |
| 35 | Advanced | "Advanced · diagnostics" disclosure (chevron.forward rotates 90°, 0.15 s easeInOut, 12/9 padding) | | | collapsed (expanded + scrolled + focused when opened from the session-key-expired notice) | — |
| 36 | Advanced *(exp.)* | claude.ai session key [+ green "Saved" / orange "Expired"] / hint (+ per-account note when extra roots) | SecureField (max 220, placeholder `••••••••` if saved else `sk-ant-sid…`) + Save (spinner while validating; disabled when empty) + Delete (if saved) | | | file in app folder (owner-only, not encrypted) |
| 37 | Advanced *(cond. orgs>1)* | Organization | picker (max 220) | | | |
| 38 | Advanced | Per additional Claude root: "Session key for <email or ~/folder>" (+ folder line if title≠folder) + field/Save/Delete + error | | | | |
| 39 | Advanced | Disable Keychain access / hint | switch | | false | `disableKeychainAccess` |
| 40 | Advanced | Refresh limit token cache / "Reads Keychain only when pressed…" | button (disabled when Keychain disabled); orange result text below | | | |
| 41 | Advanced | Additional Claude accounts / hint / "Detected: …" / textfield (`~/.claude-work`, vertical axis) / "N account folder(s) found" | committed on submit, focus loss, disappear | | "" | `ClaudeAccountRoots.defaultsKey` |
| 42 | Advanced | Additional scan folders / hint / Provider picker (registered providers) / textfield (`~/path/to/sessions`) / "Scans N extra folder(s) now" (250 ms debounce) | per-provider; committed on submit, blur, provider change, collapse, disappear | | "" | `CustomScanRoots.defaultsKey(for:)` |
| 43 | Advanced | "Token basis: totalTokens (input + output + cache, local date)" | caption2 tertiary, 12/8 padding | | | |
| 44 | About & Support | Report a problem / "Attaching the log file to the email helps a lot with diagnosis." | button | | | |
| 45 | About & Support | Show log file | button "Finder" (reveals file) | | | |
| 46 | About & Support *(cond. mail failed)* | orange selectable: "Couldn't open a mail app. Please email parkdongmin123@gmail.com directly." | | | | |

Threshold coupling (UsageStore.swift:90-101): `thresholdGap = 5`. Setting warn ≥ crit sets crit = warn+5.
Setting crit ≤ warn sets warn = crit−5. On load, `warn = min(stored, crit − 5)`.

Difficulty mapping (CompanionModel.swift:136-180):
```
range 0.1…2.0, default 1.0; clamp: non-finite → 1.0
position(v) = ln(clamp(v)/0.1) / ln(20)
value(p): p = clamp(p,0,1); if |p − position(1.0)| < 0.01 → 1.0
          v = 0.1 * 20^p ; mag = 10^(floor(log10 v) − 1) ; return round(v/mag)*mag   // 2 significant digits
display: pct = v*100; ≥10 → "%.0f%%"; ≥1 → "%.1f%%"; else "%.2f%%"
```
- Growth save rescales banked progress: `credits/oldThreshold*newThreshold`, rounded down. Rounding never
  completes a stage: when `credits < oldThreshold`, the result is capped at `newThreshold − 1`. Saving never
  evolves or hatches the Pokémon.
- Shop is a pure multiplier.
- The draft is local: leaving Settings discards it (`DifficultyDraft`, DifficultySettingsSection.swift:4-22).
- Slider row layout: label 76 pt wide, spacing 10, value 52 pt trailing. Low/high captions sit under the slider
  with insets leading 86 and trailing 62. The row is padded 12/8.

### 3.4 Save transfer (`Core/SaveTransfer.swift`)

- Envelope: `{ format: "poketokenbar.save", schema: 2, appVersion, exportedAt (ISO8601), sourceDevice, state }`.
  JSON is pretty-printed with sorted keys (:99-111).
- Decode (:113):
  - size > **8 MiB** → `fileTooLarge`;
  - header decode fails or format ≠ id → `notASaveFile`;
  - `schema > 2` → `newerSchema`;
  - full decode fails → `notASaveFile`;
  - then `sanitized`.
- Sanitize (:143):
  - Clamp token ints to `[0, 1e18]`: `usedSinceInstall`, `spentTokens`, `eggUsage`,
    `claimedTodayTokensByProvider`, and `eggPrayers.count`.
  - When an active Pokémon exists, drop `eggTier`, `pendingHatchID` and the prayers.
  - Drop an `eggTier` whose `captureRateCeiling` is nil.
  - Clamp the active Pokémon's `usedAtStage`, `totalForms` (1…12) and `stageIndex`, dedupe `pathIDs` and
    `plannedPathIDs` (first occurrence wins), and re-locate `stageIndex` by the current ID.
  - Dedupe dex `chainOrder`, sanitize the profiles, and run `reconcileRepresentativeSelection`.
- Rebase on import (:211):
  - Keep the **current** `language`.
  - Merge `candyGrantTier` per key, taking the max.
  - Merge `candyWindowEpoch`: both tiers > 0 → max epoch; one side > 0 → that side's epoch; neither → max.
  - `candyFeatureSeeded` = OR of both sides.
  - If `hasUsageData && !todayByProvider.isEmpty`: `installBaselineSet = true`,
    `claimedTodayTokensByProvider = today`, `lastDate = today`.
  - Otherwise: `installBaselineSet = false`, `claimed = nil`, `lastDate = ""`.
- File names: export is `PokeTokenBar-Save-yyyy-MM-dd.json`. The pre-import backup is
  `companion-state.pre-import-yyyy-MM-dd-HHmmss.json` in the state dir, **5 kept**. Backups sort
  lexicographically and the oldest are deleted. Stamps use the `en_US_POSIX` locale and **local** time.
- Apply (CompanionStore.swift:2014): back up first (on failure, throw `backupFailed` and leave the state
  untouched), then rebase, invalidate in-flight work (generation += 1, clear line, events, celebration, candy and
  mint feedback), set `displayState` to idle or egg, run migrations, save, and reload the line and profiles.
- Confirm dialog (SettingsView.swift:1074):
  - Warning style; title "Replace this Mac's progress?"; body = incoming dex/tokens, exported-at and device,
    current dex/tokens, and the replacement note mentioning that 5 backups are kept.
  - Buttons are `[Replace, Cancel]` and **Return = Cancel** (`ImportConfirmPolicy`).
  - The success alert reads "Imported — N in Pokédex · X lifetime".
  - Error messages: notASaveFile → "That isn't a PokeTokenBar save file."; newer → "This save was made by a newer
    version — update the app and try again."; tooLarge → "That file is too large to be a save — it looks like the
    wrong file."; backupFailed → "Import stopped because the current state couldn't be backed up — your progress
    is untouched. Check free disk space."
- Export: show a save panel with the suggested name, `.json` filter and directory creation allowed. Write the
  file atomically, then reveal it in Finder. A failure shows an alert.

### 3.5 Snapshots (`Core/SaveSnapshots.swift`)

- Directory: `<stateDir>/.snapshots/companion-state.json/` (a folder named after the state file, :29).
- File name: `companion-snapshot-yyyy-MM-dd-HHmmss.json`. On collision, append `-1`, `-2`, … before `.json`
  (:144-151).
- The content is the same envelope as an export (`SaveTransfer.encode`).
- `maxSnapshotsToKeep = 10`. Pruning happens after every create and drops everything after the 10 newest
  (:168).
- List (:62):
  - Consider only files with the prefix and a `.json` suffix.
  - Decode each as an envelope, or as a raw `CompanionState` (legacy). For a legacy file the date is parsed from
    the file-name stamp (a trailing `-N` counter is stripped), then the creation date, then now.
  - Undecodable files are skipped silently.
  - Sort by date descending, then by id (file name) descending.
- Auto snapshot (CompanionStore.swift:2096):
  - Runs at the end of every `update()` tick, when there is no snapshot yet or the newest is ≥ **12 h** old.
  - Requires `usedSinceInstall > 0 || !dex.isEmpty || active != nil`.
  - Failure is silent.
- Manual: "Create snapshot" shows the alert "Snapshot created" or the error.
- Restore (:2112):
  - Load and validate the target **first**, because the safety snapshot could prune it.
  - Then take a safety snapshot of the current state (`try?`), `applySave` (which also writes a pre-import
    backup), and refresh the list.
  - Confirm dialog: title "Restore this snapshot?"; body = target dex/tokens/date, current dex/tokens, and "Your
    current state will be backed up as a new snapshot before restoring." Buttons are `[Restore, Cancel]`, with
    Return = Cancel. Success shows "Restored — N in Pokédex · X lifetime".
- The list is refreshed when Settings appears (`companion.refreshSnapshots()`).

### 3.6 Update checker (`Core/UpdateChecker.swift`)

- Request: `GET https://api.github.com/repos/chattymin/PokeTokenBar/releases/latest` with header
  `Accept: application/vnd.github+json` and a 15 s timeout. Status 200 is required. Read `tag_name` and
  `html_url`.
- Trust: the URL must be `https` with host exactly `github.com`.
- Tag: trim; strip a single leading `v`/`V`. It must have exactly 3 dot-separated parts, each 1–9 ASCII digits,
  normalized through `Int` to drop leading zeros. Anything else, prereleases included, is treated as a failed
  check.
- Cooldown: `check(minInterval: 1800)` applies only after a **successful** check. Failures do not start a
  cooldown. An in-flight guard prevents concurrent checks. The manual "Check now" passes `minInterval: 0`.
- When a check runs: once at launch, on popover open (default 1800 s cooldown), and manually.
- `consider`: not newer → clear both. Equal to the stored `skippedUpdateVersion` → it goes to `skipped` (Settings
  only). Otherwise → `available` (banner).
- `isNewer`: numeric compare part by part; a missing part counts as 0.
- Skip (`skipCurrent`) stores the version string. "Show again" removes the key and moves skipped back to
  available.
- The banner shows "🆕 vX available (you have Y)" with Update / Skip this version, gated by
  `updateNotificationsEnabled`.
- Apply: if `brew list --cask poke-token-bar` succeeds within 20 s, a detached `/bin/sh` script waits for the
  pid, runs `brew update; brew upgrade --cask poke-token-bar` (300 s watchdog), relaunches through
  `launchctl kickstart` or `open`, and the app terminates. Otherwise it opens the release page.

### 3.7 Localization (`Core/Localization.swift`, 1981 lines)

- **8 languages**: `ko, en, ja, es, fr, pt, de, ru` (`AppLanguage`, CompanionModel.swift:10). Labels: 한국어,
  English, 日本語, Español, Français, Português, Deutsch, Русский.
- Default language: `Locale.preferredLanguages.first` prefix(2) in {ko, ja, es, fr, pt, de, ru}; anything else →
  en.
- PokéAPI name codes: ko→[ko], en→[en], ja→[ja-hrkt, ja], es, fr, pt→[pt-br, pt], de, ru (ru currently falls back
  to en).
- Mechanism: `struct L { func t(ko,en,ja,es,fr,pt,de,ru) }`. Every key is a computed var or func with 8
  positional strings.
- **424 keys** (vars and funcs) in `Localization.swift`, plus `LocalizationErrors.swift` (`userFacingError`,
  `importErrorMessage`, …). That gives roughly **3,400 strings** in total (424 × 8).
- MARK groups:
  - tabs, header, limits, footer, usage recap, settings, representative, difficulty, floating pet, session key,
    session-key expiry, Keychain help, additional Claude accounts;
  - save transfer, snapshots, report a problem, companion, dex header, Pokémon detail, companion notifications,
    prayer, region dex, limit-token errors, Claude 401, Antigravity 401;
  - OpenCode Go, Grok, Cursor limits, updates, notifications, bag/items, shop, candy notifications.
- Keys used in this area: see §3.3 labels, plus `floatingPetMenuOpen` ("Open Token Bar"), `floatingPetMenuHide`
  ("Turn off floating pet"), `backViewLabel`, `floatingPetHeartsLabel` ("Your pet is happy", accessibility),
  `floatingPetHover*`, `agent*`, `prayerHoverLine`, `prayerQuip`, `notif*`, `import*`, `restore*`,
  `snapshot*`, `report*`, `update*`.

### 3.8 Crash reporter, log, single instance, login item, paths

- **AppLog** (AppLog.swift):
  - File: `~/Library/Logs/PokeTokenBar.log`. Line format `[<ISO8601 UTC>] <message>\n`.
  - Writes are async on a serial queue.
  - Rotation: when the size exceeds 2 MiB before a write, delete `PokeTokenBar.old.log`, rename the current
    file to it, then start fresh.
  - Only the bundled app writes.
  - `writeIfChanged(key, msg, repeatAfter: 3600)` suppresses repeated identical status lines.
  - `writeAndFlush` is used on exit paths.
- **CrashReporter** (CrashReporter.swift):
  - At launch: merge `~/Library/Logs/PokeTokenBar.crash.log` into the main log as "previous session crash
    record: …", then delete it.
  - If the marker `PokeTokenBar.running` exists, log "previous session did not exit cleanly".
  - Write the marker, then log "launch: PokeTokenBar <ver> started".
  - Install handlers for SIGSEGV, SIGABRT, SIGBUS, SIGILL, SIGFPE and SIGTRAP. Each writes a static line to a
    pre-opened fd on crash.log, then re-raises with the default handler. An NSException handler writes "[CRASH]
    uncaught exception: name — reason".
  - On `willTerminate`: delete the marker and log "clean shutdown" (flushed).
  - `SIGPIPE` is ignored app-wide (PokeTokenBarApp.swift:93).
- **SingleInstance**:
  - Order: the yield check runs first, then `CrashReporter.install`, so a yielding process never touches the
    marker.
  - Rule: yield when **any other instance started strictly earlier**, judged by kernel start time. An unknown
    own start time or an equal start time → stay.
  - Exit is clean (exit 0).
- **LoginItem**: `SMAppService.agent(plistName: "io.github.chattymin.poketokenbar.login.plist")` with
  `KeepAlive.SuccessfulExit=false` and `ThrottleInterval 10`. Login launch and crash relaunch share one toggle.
  A one-time migration from the legacy `SMAppService.mainApp` registers the agent first and then unregisters the
  legacy item.
- **AppStatePaths**: `$PTB_STATE_DIR` (trimmed, non-empty), otherwise `~/Library/Application
  Support/PokeTokenBar/`. Created on access. The state file is `companion-state.json`.
- A one-time legacy migration moves `Application Support/TokenMac` → `PokeTokenBar` when only the old folder
  exists.

---

## 4. Data / persistence

| Item | macOS location | Format |
|---|---|---|
| UI/settings prefs | `UserDefaults.standard` keys listed in §3.3 + `floatingPetOriginX/Y`, `skippedUpdateVersion`, `disabledProviderIDs`, `growthDifficulty`, `shopDifficulty`, per-provider scan roots, `NSInitialToolTipDelay` (registered) | plist |
| Companion state (incl. `language`, `eggPrayers{count}`, representative) | `<stateDir>/companion-state.json` | JSON (lenient decode) |
| Import backups | `<stateDir>/companion-state.pre-import-<stamp>.json` ×5 | raw CompanionState JSON |
| Snapshots | `<stateDir>/.snapshots/companion-state.json/companion-snapshot-<stamp>[-n].json` ×10 | envelope JSON |
| Export | user-chosen, default `PokeTokenBar-Save-<yyyy-MM-dd>.json` | envelope JSON |
| Logs | `~/Library/Logs/PokeTokenBar.log` (+`.old.log`), `PokeTokenBar.crash.log`, `PokeTokenBar.running` | text |
| Session keys | app folder, owner-only file | — |
| Not persisted | agent tracker, run phase, prayer-refresh milestone, hearts, event pop, `lastObservedUsage` | memory |

---

## 5. UI spec

### 5.1 Colour tokens (macOS semantic colours → hex)

| Token | Light | Dark |
|---|---|---|
| windowBackground (bubble fill, hover callout) | `#ECECEC` | `#323232` |
| controlBackground (settings cards) | `#FFFFFF` | `#1E1E1E` |
| separator (card border @60 %, dividers) | `rgba(0,0,0,0.10)` | `rgba(255,255,255,0.10)` |
| label | `rgba(0,0,0,0.85)` | `rgba(255,255,255,0.85)` |
| secondaryLabel | `rgba(0,0,0,0.50)` | `rgba(255,255,255,0.55)` |
| tertiaryLabel (hints) | `rgba(0,0,0,0.26)` | `rgba(255,255,255,0.25)` |
| accent (Back button, prominent Save) | `#007AFF` | `#0A84FF` |
| red (critical bubble title, login error) | `#FF3B30` | `#FF453A` |
| orange (update found/skipped, key expired, errors) | `#FF9500` | `#FF9F0A` |
| green ("Saved") | `#28CD41` | `#32D74B` |
| Settings window background (screenshots, dark) | — | `#1E1E1E` popover / cards `#2A2A2A`–`#2C2C2C` |

Heart pixel art uses the same colours in both modes: O `#202020`, W `#FFFFFF`, R `#E0303C`, D `#A8202C`,
H `#FF9AA2`. The grid is 17×16 cells at 2 pt per cell (34×32 pt), drawn from `HeartBubbleArt.rows`
(FloatingPetPanel.swift:1309-1326). Copy the rows verbatim.

### 5.2 Fonts (macOS → Windows)

The macOS system font is SF Pro. Use **Segoe UI Variable** (falling back to Segoe UI) at the same pt→px values
(1 pt = 1 DIP).

- headline: 13 semibold;
- body: 13;
- callout: 12;
- caption: 10;
- caption2: 10;
- bubble title: 11 bold;
- bubble body: 10;
- hover callout: 11;
- value labels use monospaced digits (`font-variant-numeric: tabular-nums`).

### 5.3 Floating pet component tree

```
PetWindow (transparent, borderless, always-on-top, no taskbar, no focus, no shadow)
 └ VStack(spacing 8, bottom-aligned, fills panel)
    ├ SpeechBubble?  (width 164 content + 2×8 pad, 6 pt vertical pad, radius 12 continuous,
    │                 fill windowBackground, shadow black 15 % r4 y+2, tail triangle (10,0)-(20,0)-(15,6)
    │                 at bottom-left (x 10–20), offset y+5, then 6 pt bottom pad)
    │   ├ Title 11 bold, red if critical else label
    │   └ Body 10 regular secondaryLabel, ≤2 lines
    └ Sprite (petSize², aspect-fit, nearest-neighbour pixel art, back sprite when facesBack)
        ├ wobble rotation (prayer)
        ├ overlay PrayerEmojiLayer (particles inside sprite box, no hit testing)
        ├ overlay HeartBubble (top-aligned, y = round(visibleTop + 3 − 32))  // visibleTop = first opaque row of the median frame, central band 35–65 % width, alpha > 16
        └ AgentBob scale modifier (anchor bottom)
HoverCallout (separate click-through window, shadow on)
 └ padding 8, radius 8, border 0.5 separator, bg windowBackground, text 11 label, multi-line
ContextMenu: [Open Token Bar] [Turn off floating pet] [✓ Show back view]
```
- States:
  - idle shows the sprite only;
  - hover shows the callout, unless a bubble or hearts are showing;
  - bubble/hearts grow the panel upward to `(max(size,180), size+72)` while the pet stays put;
  - the panel is hidden when the pet is disabled or the display is asleep.
- The sprite frame rate is capped by Animation quality (floor 0.4, 0.2 or 0.1 s). Low Power forces 0.4 and a
  static (non-animated) host.

### 5.4 Settings layout

- Container: popover width **360**, Settings height **460**. A fixed header and a fixed footer sit around a
  scrolling body. Dividers separate them.
- Header (padding 14/10):
  - Leading: "‹ Back" (`chevron.backward` + text, accent colour, plain style). Esc triggers it.
  - Centre: "Settings" in headline.
  - Trailing: an invisible "Back" for balance.
- Body:
  - VStack with spacing 18 and padding 16.
  - Section: an uppercase title (caption, semibold, secondary, leading pad 4), then 6 pt, then a card (radius 8,
    fill controlBackground, 1 px stroke separator @60 %). Inside the card the rows are separated by full-width
    dividers.
  - Row: HStack with spacing 10, padding 12 horizontal / 8 vertical, min height 38.
  - A toggle row has a VStack (label + caption2 tertiary hint, spacing 1), a spacer, and a small switch.
- Footer (padding 16/10, caption2, tertiary, spacing 5): `v2.5.7 · GitHub · Website · ♥ Sponsor`. The links
  are underlined plain buttons with the URL as tooltip:
  - `https://github.com/chattymin/PokeTokenBar`
  - `https://chattymin.github.io/PokeTokenBar/`
  - `https://github.com/sponsors/chattymin`
- Alerts are modal NSAlerts with a "Close" button. The app activates first.
- Loading states:
  - the Check now / Save key / Refresh token buttons swap to a small spinner and become disabled;
  - the custom scan count is debounced by 250 ms.

---

## 6. Windows adaptation

| macOS API / behaviour | Windows / Tauri equivalent |
|---|---|
| `NSPanel` borderless, `.nonactivatingPanel`, `.floating`, `canJoinAllSpaces`, `fullScreenAuxiliary`, `hidesOnDeactivate=false` | Tauri `WebviewWindow` with `transparent`, `decorations:false`, `alwaysOnTop`, `skipTaskbar`, `shadow:false`, `focus:false`; add `WS_EX_NOACTIVATE` (via `windows` crate `SetWindowLongPtrW(GWL_EXSTYLE)`) so clicks don't steal focus. Virtual desktops: `visibleOnAllWorkspaces: true`. |
| Panel grows upward for bubble | Resize the window and move its `y` by the height delta (Windows is y-down: keep the **bottom** edge fixed). Persist the pet's bottom-left: store `{x, bottom}` in logical px, converting between y-down and y-up. |
| Hover callout as second click-through `NSPanel` | Second transparent window (`label: "pet-callout"`) with `set_ignore_cursor_events(true)`, or render the callout inside the pet window (needs extra headroom; simpler but grows the window). |
| `mouseMoved` tracking with no button (stroke) | DOM `pointermove` with `e.buttons === 0`, using `e.screenX`. |
| Custom drag with click threshold | Keep the current `startDragging()` after 4 px slop. Persist origin on `WindowEvent::Moved` only when phase is home (Rust side). Replace `tauri-plugin-window-state` for the pet, which persists the window rect rather than the pet origin and fights bubble resizing. |
| `NSEvent.mouseLocation` + `NSScreen.visibleFrame` | `app.cursor_position()` + `app.monitor_from_point()` / `available_monitors()`, then `monitor.work_area()` (Tauri ≥2.1), which excludes the taskbar. |
| `NSAnimationContext` frame animation 0.6 s | Rust thread or JS `requestAnimationFrame` interpolating `setPosition`/`setSize` with an ease-in-out cubic at about 60 Hz. Keep a generation counter to cancel. |
| `NSMenu.popUpContextMenu` | `@tauri-apps/api/menu` `Menu.new` + `popup()` (already used); `CheckMenuItem` for "Show back view". |
| Native tooltip (`toolTip`) | `title` attribute (already used), but only when no callout or hearts show; Windows tooltip delay is fixed at about 500 ms. |
| Low Power Mode | `GetSystemPowerStatus().SystemStatusFlag & 1` (Battery Saver on), polled or via `RegisterPowerSettingNotification(GUID_POWER_SAVING_STATUS)`. |
| Reduce Motion | `SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION)` == FALSE, or CSS `prefers-reduced-motion: reduce` in the webview (WebView2 honours the Windows "Animation effects" setting). |
| Display asleep (`screensDidSleep`) | `RegisterPowerSettingNotification(GUID_CONSOLE_DISPLAY_STATE)`; hide the pet and pause polling. Minimum viable: skip. |
| `poketokenbar://` via Apple Event `kAEGetURL` | `tauri-plugin-deep-link` with `{"deep-link": {"desktop": {"schemes": ["poketokenbar"]}}}` and `register_all()` at startup (registry `HKCU\Software\Classes\poketokenbar`). Windows launches a **new process** with the URL in argv, so combine with `tauri-plugin-single-instance` feature `deep-link`: the single-instance callback receives `args` and forwards the `poketokenbar://` arg to the handler without revealing the main window. Never show or focus windows for agent URLs. A cold start from a URL should exit when the app is not running, mirroring the README's `pgrep` guard; or let it start and drop the event. |
| Hook script `open -g URL` | Document `start "" "poketokenbar://agent?state=done&label=x"` (cmd) or `Start-Process 'poketokenbar://…'` (PowerShell). Guard with `tasklist /FI "IMAGENAME eq PokeTokenBar.exe" \| find /I "PokeTokenBar"`. |
| `UNUserNotificationCenter` | `tauri-plugin-notification` (already used). Identifier dedupe is not supported: rely on the tier map. |
| `NSStatusItem` title text | Not supported on Windows. Use the tray tooltip (max 127 chars) and optionally render the text into the tray icon bitmap. "Show in menu bar" toggles control the tooltip. |
| `SMAppService` LaunchAgent + KeepAlive crash relaunch | `tauri-plugin-autostart` (HKCU `Run` key) for login. For crash relaunch, add `RegisterApplicationRestart(L"", RESTART_NO_REBOOT)` (WER restarts after a crash or hang if the app ran ≥ 60 s), or a tiny watchdog. Clean exit does not restart. |
| `SingleInstance` (later yields) | `tauri-plugin-single-instance` (already used; the later instance exits and forwards args). Same semantics. |
| `NSSavePanel` / `NSOpenPanel` | `tauri-plugin-dialog` `save({defaultPath, filters:[{name:"JSON",extensions:["json"]}]})` / `open({multiple:false, filters})`. |
| `NSAlert` (Return = Cancel) | `tauri-plugin-dialog` `ask()`/`confirm()`, or an in-app modal with Cancel as the default-focused button. The native `MessageBoxW` default button can be set to Cancel only through a custom modal. Prefer the in-app modal for exact text. |
| `activateFileViewerSelecting` (Finder reveal) | `tauri-plugin-opener` `revealItemInDir(path)` (explorer `/select,`). |
| `mailto:` via `NSWorkspace.open` | `opener.openUrl("mailto:…?subject=…&body=…")`. On failure show the fallback address. |
| `~/Library/Logs/PokeTokenBar.log` | `app.path().app_log_dir()` → `%LOCALAPPDATA%\com.tkdals2317.poketokenbar\logs\PokeTokenBar.log`, or reuse `app_data_dir`. Same 2 MiB, one-generation rotation. |
| POSIX signal handlers + `.ips` | Rust `std::panic::set_hook` (exists: `crash.log`), plus a running-marker file for unclean-exit detection, plus a merge of crash.log into the main log on the next launch. Native faults: optional `SetUnhandledExceptionFilter`. |
| `Application Support/PokeTokenBar` + `PTB_STATE_DIR` | `app_data_dir()` (`%APPDATA%\com.tkdals2317.poketokenbar`), honouring the `PTB_STATE_DIR` env override. |
| Keychain (session-key area) | Windows Credential Manager via the `keyring` crate, or DPAPI-encrypted file (`CryptProtectData`). The macOS app stores the session key as an owner-only plain file, so a DPAPI file is strictly better. |
| `Host.current().localizedName` (device name) | `COMPUTERNAME` env var or `GetComputerNameExW(ComputerNameDnsHostname)`. |
| Homebrew in-place update | `tauri-plugin-updater` (already used, signed `latest.json`, passive NSIS). Keep the GitHub-latest version check semantics (skip version, 30 min cooldown on success only) on top. |
| UserDefaults | `settings.json` in `app_data_dir` (exists), with `#[serde(default)]` per field for lenient load. |

---

## 7. Gaps vs current Windows code

Current files: `src/Pet.tsx` (75 lines), `src/Settings.tsx` (239), `src-tauri/src/settings.rs` (131),
`src-tauri/src/lib.rs` (421), `src/i18n.ts` (ko/en/ja only).

### Floating pet (`Pet.tsx`, `lib.rs`)
- G1. Click always toggles the main window. Missing: **pray on egg** (no `eggPrayers` in `companion.rs`), the 🙏
  particles, the wobble, and the 30-prayer refresh throttle.
- G2. No hover callout: only an `<img title>` showing `name · today N`. The macOS text is "Today: N tokens (limit
  P%)", plus sulk, event and prayer lines.
- G3. No limit-alert speech bubble, and no `floatingPetBubbleAlerts` setting.
- G4. No stroking/hearts (`PettingDetector`, the heart pixel art, 1.8 s).
- G5. No sulk/back view. There is no 3 h idle → back sprite, no `alwaysFacesBack` setting, no back sprite URL
  usage, and no sulk hover suffix.
- G6. No event pop callout (6 s) for hatch/evolve/graduate.
- G7. No agent URL scheme at all: no deep-link plugin, tracker, bob, waiting/done bubbles or run-to-cursor.
- G8. Context menu differs:
  - macOS: Open Token Bar / Turn off floating pet / ✓Show back view;
  - Windows: Open window / Size ▸ (48, 72, 96, 120, 160, 224, 288, 384) / Hide pet / Quit.
  - Decide whether to keep Size and Quit (Windows extras) or match exactly.
- G9. Pet size:
  - the default is 120 (`tauri.conf.json`, `petSize()` fallback) instead of **96**;
  - the size is not a setting: it is read back from the window size and persisted by window-state;
  - on macOS it is a persisted setting (`floatingPetSize`) and the Size slider appears only when the pet is
    enabled.
- G10. `pet_enabled` defaults to **true** (`settings.rs:36`) while macOS defaults to **false** (opt-in). The
  `tauri.conf.json` pet window also has `visible:true`.
- G11. Window geometry: the window is fixed square with no headroom for bubbles and no bottom-anchored growth.
  Window-state persists the rect, not the pet origin. There is no off-screen fallback to bottom-right (24 px
  inset).
- G12. No `WS_EX_NOACTIVATE`: clicking the pet can steal focus.
- G13. No Low-Power/Reduce-Motion gating, and no animation-quality frame cap (sprites are static PNG/GIF
  `<img>`; check `sprite()` in `shared.ts`).

### Settings (`Settings.tsx`, `settings.rs`)
- G14. Section order differs. Windows puts Updates first. macOS order is: General, Difficulty, Show in menu bar,
  Tracked providers, Floating Pet, Notifications, Updates, Backup & Transfer, Advanced, About & Support, and the
  footer.
- G15. Languages: 3 versus 8. es/fr/pt/de/ru are missing (`settings.rs:5`, `i18n.ts`), and the fallback is en.
- G16. Refresh intervals:
  - Windows offers `[1,2,5,10,15,0]` minutes and defaults to **1**;
  - macOS offers Manual, 1, 2, 5, 15 and defaults to **2 min**;
  - 10 min is a Windows extra, and the order differs (Manual is first on macOS).
- G17. Missing General rows: Animation quality (powerSaver/balanced/smooth, default powerSaver), Daily trend
  period (7d/14d/21d/month, default 14d), and Tracked Claude account. The Representative row on Windows is a
  "★ name ✕" button. macOS uses a menu with "Follow current companion ✓" and "Choose in Pokédex…".
- G18. Difficulty:
  - only Growth exists; **Shop prices** is missing (`shopDifficulty`);
  - the value label is `×1.0` instead of `100%`;
  - Windows shows the low/high labels as a hint ("faster ← → slower") instead of "Easy … Hard" under the slider;
  - the growth-rescale-on-save rule needs verifying in `companion.rs` `set_difficulty`.
- G19. "Tray" section has only tokens and limit. Missing: Today's cost ($), Pokémon growth %, Percent color
  (gauge/attention/off), and the "All off shows only the character" footer.
- G20. Tracked providers section missing. The Windows backend reads only claude and codex.
- G21. Floating Pet section missing: Show back view, Show notifications as bubbles, React to agent activity, and
  the pet hint text.
- G22. Notifications:
  - "Provider status checks" is missing;
  - threshold coupling differs: Windows `sanitized()` sets `crit = max(crit, warn)` with no 5-point gap and
    never pushes warn down; macOS keeps `crit − warn ≥ 5` in both directions.
- G23. Updates:
  - no "Update notifications" toggle;
  - no skip-version / show-again flow;
  - no orange "Version X is available" result row;
  - the Windows updater uses the signed `latest.json` (fine), but the macOS semantics (success-only 30 min
    cooldown, skipped kept in Settings) are absent.
- G24. Backup & Transfer missing entirely: export/import envelope, pre-import backups ×5, snapshots ×10, auto 12 h,
  restore.
- G25. Advanced missing entirely: session key(s), Disable Keychain, Refresh limit token, Additional Claude
  accounts, Additional scan folders, aggregation note.
- G26. About & Support and the footer are missing: Report a problem (mailto), Show log file, and the version,
  GitHub, Website and Sponsor links. The current version is shown in the Updates row instead.
- G27. Launch-at-login: no "bundled only" hint (N/A on Windows) and no crash relaunch.

### Services
- G28. Logging: there is no general app log. Only the panic `crash.log` exists. Missing: the 2 MiB rotation, the
  running-marker unclean-exit detection, and merging crash.log into the main log.
- G29. Single instance exists. The difference: the forwarded-args callback reveals the main window. It must
  ignore `poketokenbar://` args, which should go to the agent handler without UI.
- G30. Settings persistence key names differ from macOS (snake_case JSON, e.g. `pet_enabled`, `tray_tokens`).
  That is acceptable because there is no cross-platform settings file. Save envelopes, however, **must** use
  the macOS `CompanionState` JSON field names to be importable across platforms. This needs verification against
  `companion.rs` serialization (currently `companion.json` has its own schema).
- G31. No `PTB_STATE_DIR` override. The state file is named `companion.json`, versus `companion-state.json` on
  macOS.

---

## 8. Open questions

1. Cross-platform save import: should a Windows export be importable on macOS and the reverse? If yes, the
   Windows `companion.rs` state must serialize to the macOS `CompanionState` schema (field names, the ISO8601
   `exportedAt`, schema 2). If no, use a distinct `format` id to avoid the "progress wiped" confusion that the
   envelope exists to prevent.
2. Hover callout: should it be a separate click-through window (exact macOS behaviour, no window growth) or an
   in-window element? This affects how much headroom the pet window needs.
3. Context menu: keep the Windows-only "Size ▸" and "Quit" items or match macOS exactly? With no menu-bar
   status item to quit from, Quit may be needed in the tray menu only.
4. Agent URL on cold start: should the app launch when it is not running? On macOS the README hook prevents this
   with `pgrep`. Decide whether the Windows handler exits immediately when invoked with only a deep link and no
   running instance.
5. Screenshot `floating-pet.gif` shows a compact "12.4M" in the hover text, but v2.5.7 code uses
   `TokenFormatter.grouped` (e.g. "12,400,000"). Follow the code.
6. Crash relaunch: is `RegisterApplicationRestart` acceptable? It only fires after the app has run for 60 s and
   it relaunches through WER.
7. Which providers are in scope for "Tracked providers" on Windows (16 on macOS)? Rows should list only
   providers the Windows backend can read.
8. Session-key storage: use Credential Manager (`keyring`) or a DPAPI file? The macOS app uses a plain
   owner-only file.
