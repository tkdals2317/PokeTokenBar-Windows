# Parity spec: Home tab usage data and charts

Source of truth is PokeTokenBar v2.5.7 (Swift, macOS). Paths below are relative to `Sources/PokeTokenBar/`.
The target is the Windows port in this repo: Tauri 2, with Rust in `src-tauri/src` and React/TS in `src`.

## 1. Overview

The Home tab of the macOS popover shows, from top to bottom (`UI/PopoverView.swift:149-188`):

1. Companion header (sprite, name, growth bar). The only part covered here is its placement. Its full spec lives in the companion area.
2. A `Divider`.
3. **Usage header** (`PopoverView.swift:193-257`):
   - Today's total, shown compact and grouped, plus today's cost.
   - This-week and this-month totals and cost.
   - Streak badge and the "Details" recap button.
   - The activity panel: daily bars or a 16-week heatmap.
   - Provider tabs, then the selected provider's input/output/cache breakdown and per-model breakdown.
4. `Divider`.
5. Provider status banner. It appears only during an incident.
6. **Limits section** for the selected provider (only if it has limits), with gauge, pace marker, reset label and forecast. Then a `Divider`.
7. Footer, outside the scroll area: refresh button, "Updated N sec", error triangle, pet toggle, settings, quit.

Items 1-6 sit in a vertical `ScrollView` of fixed height **520 pt** (`PopoverView.swift:184`). The scroller lane is reserved on the right.

The "Details" button replaces the whole popover with the **Recap screen** (`UI/UsageRecapView.swift`). The recap has week/month/year scopes and a Pokédex-style card.

All numbers come from `UsageStore` (`Core/UsageStore.swift`). The store merges per-provider `ProviderSnapshot`s (`Core/Models.swift:832-852`), which local log parsers produce. The recap and streak read a persistent day ledger (`Core/UsageRecap.swift`).

## 2. Features

1. **Today total.** Compact number at 28 pt bold. Next to it, the grouped exact number appears when the total is 1,000 or more. Today's cost is right-aligned.
2. **Week and month totals.** "This week 3.2M $7.73   This month 3.2M $7.73". When there is not enough width, this collapses to two rows.
3. **Cost provenance.** Each cost is source-reported, estimated, partial or unavailable. An unavailable cost reads "Unavailable", or "$—" in compact form.
4. **Model price table.** Used for estimates, with long-context multipliers.
5. **Streak badge.** A flame plus a day count, shown from 2 days. The flame is orange when today counts and gray while today has no usage yet.
6. **Recap button.** A "Details" capsule that opens the Recap screen.
7. **Activity panel, view 1: daily bars.** Windows are 7, 14 or 21 days, or this month; the default is 14 days. Bars stack by provider when two or more providers were used. Weekend ticks, axis labels, the peak, a hover tooltip and a legend come with it.
8. **Activity panel, view 2: 16-week heatmap.** GitHub contribution-style greens with weekday labels. The caption readout follows hover.
9. **View cycling.** Clicking anywhere on the panel switches to the next view. Page dots and a tooltip hint at this.
10. **Recap screen.**
    - Scopes: Week, Month, Year. Previous, current and next navigation.
    - Total and a delta chip compared with the same elapsed days of the previous period.
    - 8-segment meters per bucket.
    - Best day, best streak, active days and graduates count.
    - A strip of graduated Pokémon.
    - A best-day sentence and a "compared with the same days" note.
11. **Provider tabs.** Capsule tabs, shown when two or more snapshots exist. The selection survives reopening the popover.
12. **Provider detail row.** Name, today's tokens and cost. Input/Output on one line, Cache write/Cache read on the next. A per-model list appears when the provider reports two or more models.
13. **Limits section.** Rows have a name, relative reset time with a clock suffix, and a percentage in used or remaining mode. The gauge is colored by pace tier and carries a pace marker and a tooltip. The 5-hour forecast row, stale badge and "Load official limits" row are part of it.
14. **Footer.** Refresh, "Updated N sec" as live relative time, an error triangle with a tooltip, the pet show/hide toggle, Settings and Quit.
15. **Number formatting.** Compact K/M/B, grouped by system locale, `$%.2f`, compact cost, percent with ".0" stripped.
16. **Day ledger persistence.** Survives month boundaries. Holds this year and last year.

## 3. Rules and constants

### 3.1 Number formatting (`Core/TokenFormatter.swift`)

`compact(Int)` (`:5-19`) uses the absolute value `v` and prefixes "-" for negatives:

| range of v | output | decimals |
|---|---|---|
| `< 1_000` | `"\(value)"` (raw integer, with sign) | 0 |
| `< 999_950` | `v/1_000`, `%.1f`, trailing zeros and "." trimmed, + `K` | 1 |
| `< 999_950_000` | `v/1_000_000`, `%.1f` trimmed, + `M` | 1 |
| otherwise | `v/1_000_000_000`, `%.2f` trimmed, + `B` | 2 |

- The 999_950 promotion boundary prevents "1000.0K". For example, 999_949 gives "999.9K" and 999_950 gives "1M".
- Examples: 987 → "987", 12_345 → "12.3K", 1_000 → "1K", 190_612_940 → "190.6M", 1_240_000_000 → "1.24B", 2_000_000_000 → "2B".
- `trim` removes trailing `0`s and then a trailing `.` (`:54-59`).

The other formatters:

- **`grouped(Int)`** (`:27-32`): `NumberFormatter` `.decimal` with `Locale.current`, which is the system region and not the app language. en/ko/ja give `253,412,890`, es/de give `253.412.890`, fr/ru give `253 412 890` (NBSP).
- **`cost(Double)`** (`:34-36`): `String(format: "$%.2f")`, for example `$2.76`. It is always USD and always has two decimals, with no locale grouping.
- **`costCompact(Double)`** (`:40-46`), used by the menu bar:
  - `t = "%.1f"`. If `Double(t) < 100`, the result is `"$"+t` (for example `$9.5`, `$0.0`).
  - Otherwise `w = "%.0f"`. If `Double(w) < 10_000`, the result is `"$"+w` (for example `$311`; 99.96 gives `$100`).
  - Otherwise the result is `"$%.1fK"` of `usd/1000` (for example `$12.3K`).
  - Thresholds are judged on the rounded string.
- **`percent(Double)`** (`:49-52`): `"%.1f"`, dropping a trailing ".0", then "%". For example 79.96 → "80%" and 88.35 → "88.3%". Swift uses round-half-even on the binary value, so match `toFixed(1)` behaviour carefully.

### 3.2 Cost model

**`CostCoverage`** (`Core/UsageCost.swift:5-21`) holds three flags: `reported`, `estimated`, `unknown`. `merge` ORs each flag. `hasKnown = reported || estimated`.

**`UsageCost.text(l, compact:)`** (`:32-35`):
- If `unknown && !hasKnown`, the result is `"$—"` in compact form, or `l.costUnavailable` otherwise ("Unavailable" / "계산 불가" / "計算不可").
- Otherwise the result is `cost()` or `costCompact()`.
- A partial cost (`unknown && hasKnown`) still prints the known amount.

`explanation(l)` (`:37-40`) chooses among the partial, unavailable, estimate and reported hint strings (`Localization.swift:30-33`). No v2.5.7 view calls it, so it is not rendered.

**Per-entry cost** (`Core/LocalUsageReader.swift:52-73`, `Bucket.add`):
- An entry with `total == 0` adds tokens but leaves cost and coverage unchanged.
- If `explicitCost` is finite and ≥ 0, add it. Coverage becomes `.estimate` if `costIsEstimate == true`, else `.source`.
- Otherwise, if `costUnavailable != true` and `ModelPricing.estimatedCost(...)` is non-nil, add the estimate with coverage `.estimate`.
- Otherwise coverage is `.unavailable`. When `costUnavailable != true`, `ModelPricing.noteUnpriced(model)` logs once per normalized model: "Unpriced model: X — its cost shows as Unavailable until ModelPricing.table has a row".

**Claude source-reported cost** (`LocalUsageReader.swift:398-469`):
- A Claude transcript can contain `{"type":"cost-state","modelUsage":{"<model>":{"costUSD":n}},"hasUnknownModelCost":bool}`. The last such record in a file wins (`:509`).
- Model keys strip a `[...]` suffix: `claude-opus-5[1m]` becomes `claude-opus-5`.
- For each model, that cost is spread across the file's entries with that base model in proportion to `total` tokens. The last entry absorbs the rounding remainder.
- Those entries get `explicitCost = share` and `costIsEstimate = false`, which is `.source` coverage.
- Models that have cost but no entries are skipped.

**Claude assistant line** (`:568-580`):
- `type=="assistant"`, plus `message.usage`, plus `timestamp`.
- `model = message.model ?? "unknown"`.
- `id = message.id + "|" + requestId`.
- Tokens: `input_tokens`, `output_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`.

**Codex model** (`:2006-2011`): `payload.model`, falling back to `payload.turn_context.model`.

**Daily aggregates.** Aggregates of `PeriodUsage` and `DailyUsage` merge coverage across entries. When JSON decoding has no `costCoverage`, the default is `.empty` if `totalTokens == 0`, `.estimate` if `cost > 0`, else `.unavailable` (`Models.swift:33-34`).

**`showsCost`** = at least one snapshot with `reportsCost` (`UsageStore.swift:496-499`). Every provider in v2.5.7 has `reportsCost = true` (the default in `UsageProvider.swift:20`), so cost shows whenever any provider has a snapshot. A cost of `$0.00` with `.empty` coverage prints `$0.00`.

**Store totals** (`UsageStore.swift:501-528`):
- `todayUsageCost` sums `snapshot.today.usageCost` only where `today.date == todayKey`.
- `weekUsageCost` and `monthUsageCost` sum `weekTotal` and `monthTotal`.
- `todayTotalTokens` has the same date guard (`:323-327`).
- `weekTotalTokens` and `monthTotalTokens` sum over all snapshots.

### 3.3 Model price table (`Core/ModelPricing.swift:23-68`)

Prices are USD per **million** tokens. The columns are input, output, 5-minute cache write and cache read. A **0 in cache write means there is no supported write rate.** If `cacheWrite > 0` tokens exist for such a model, `estimatedCost` returns `nil` (unavailable) (`:108`).

| model key | input | output | cache write | cache read |
|---|---|---|---|---|
| claude-opus-5 | 5 | 25 | 6.25 | 0.5 |
| claude-opus-5-5 | 4 | 20 | 5 | 0.2 |
| claude-sonnet-5 | 2 | 10 | 2.5 | 0.2 |
| claude-sonnet-5-5 | 2 | 10 | 2.5 | 0.1 |
| claude-opus-4-20250514 | 15 | 75 | 18.75 | 1.5 |
| claude-sonnet-4-20250514 | 3 | 15 | 3.75 | 0.3 |
| claude-sonnet-4-5-20250929 | 3 | 15 | 3.75 | 0.3 |
| claude-opus-4-8 | 5 | 25 | 6.25 | 0.5 |
| claude-opus-4-7 | 5 | 25 | 6.25 | 0.5 |
| claude-sonnet-4-6 | 3 | 15 | 3.75 | 0.3 |
| claude-haiku-4-5-20251001 | 1 | 5 | 1.25 | 0.1 |
| claude-fable-5 | 10 | 50 | 12.5 | 1.0 |
| claude-fable-5-1 | 10 | 50 | 12.5 | 0.25 |
| gpt-6-astra | 10 | 50 | 12.5 | 1 |
| gpt-6-sol | 2 | 10 | 2.5 | 0.2 |
| gpt-6.1-sol | 2 | 10 | 2.5 | 0.1 |
| gpt-6-luna | 0.1 | 0.5 | 0.125 | 0.01 |
| gpt-5.6-sol | 4 | 20 | 5 | 0.4 |
| gpt-5.6-terra | 2 | 12 | 2.5 | 0.2 |
| gpt-5.6-luna | 0.2 | 1.2 | 0.25 | 0.02 |
| gpt-5 | 1.25 | 10 | 0 | 0.125 |
| gpt-5-codex | 1.25 | 10 | 0 | 0.125 |
| gpt-5.1 | 1.25 | 10 | 0 | 0.125 |
| gpt-5.1-codex | 1.25 | 10 | 0 | 0.125 |
| gpt-5.2 | 1.75 | 14 | 0 | 0.175 |
| gpt-5.2-codex | 1.75 | 14 | 0 | 0.175 |
| gpt-5.3-codex | 1.75 | 14 | 0 | 0.175 |
| gpt-5.4 | 2.5 | 15 | 0 | 0.25 |
| gpt-5.5 | 5 | 30 | 0 | 0.5 |
| gemini-2.5-pro | 1.25 | 10 | 0 | 0.125 |
| gemini-2.5-flash | 0.30 | 2.5 | 0 | 0.03 |
| gemini-2.5-flash-lite | 0.10 | 0.40 | 0 | 0.01 |
| gemini-2.0-flash | 0.10 | 0.4 | 0 | 0.025 |

**Aliases** (`:72-83`), applied after normalization:

| alias | resolves to |
|---|---|
| gpt-5.6 | gpt-5.6-sol |
| claude-sonnet-4 | claude-sonnet-4-20250514 |
| claude-opus-4 | claude-opus-4-20250514 |
| claude-sonnet-4-5 | claude-sonnet-4-5-20250929 |
| claude-haiku-4-5 | claude-haiku-4-5-20251001 |
| gpt-5-2025-08-07 | gpt-5 |
| gpt-5.1-2025-11-13 | gpt-5.1 |
| gpt-5.2-2025-12-11 | gpt-5.2 |
| gpt-5.4-2026-03-05 | gpt-5.4 |
| gpt-5.5-2026-04-23 | gpt-5.5 |

**Key normalization** (`:85-92`):
1. Trim whitespace and newlines, then lowercase.
2. Strip the **first** matching prefix among `openai/`, `anthropic/`, `google/`, `models/`. Only one prefix is stripped.
3. Apply the alias.

There is **no** family or substring fallback, so an unknown key is unpriced.

**Formula** (`:103-119`):
```
guard rate exists, all buckets >= 0; guard cacheWrite == 0 || rate.cacheWrite > 0
prompt = input + cacheRead + cacheWrite
longContext = key ∈ {gpt-6-astra, gpt-6-sol, gpt-6.1-sol, gpt-6-luna, gpt-5.6-sol, gpt-5.6-terra,
                     gpt-5.6-luna, gpt-5.5, gpt-5.4} && prompt > 272_000
           || key == "gemini-2.5-pro" && prompt > 200_000
inMul  = longContext ? 2.0 : 1.0
outMul = longContext ? 1.5 : 1.0
cost = (input*r.in + cacheWrite*r.cw + cacheRead*r.cr) * inMul + output*r.out*outMul
```
Long context is judged **per request (entry)**. Never apply the formula to aggregates.

### 3.4 Day ledger, streak and recap (`Core/UsageRecap.swift`)

**`UsageLedger`** (`:9-62`):

Storage:
- `tokensByDay: [yyyy-MM-dd: Int]` is stored under the UserDefaults key **`dailyTokenLedger`**.
- `coveredSince: String?` is stored under **`dailyTokenLedgerSince`**.

`merge(series)`:
- For each day with `totalTokens > 0`, store `max(old, new)`. Days never decrease.
- `coveredSince = min(coveredSince, min(series.date))`. Zero days in the series are real zeros.

`prune(before: cutoff)`:
- Drops keys `< cutoff`.
- If `coveredSince < cutoff`, sets `coveredSince = cutoff`.

`load`:
- Keeps only keys that parse as `yyyy-MM-dd` with a value `> 0`.
- Keeps `coveredSince` only if it parses.

`covers(day)` is `day >= coveredSince`, or false when `coveredSince` is nil.

**Recording** (`UsageStore.swift:661-674`):
- The ledger is recorded only after phase 2 (enrichment) of each refresh, from `monthDailyTotals`. Recording after phase 1 would make the first refresh after install produce an empty recap.
- It is then pruned before Jan 1 of **last year**, so this year and last year are kept.
- It saves only if changed.

**Current streak** (`:83-97`):
- Uses the Gregorian calendar, the local TZ, and `firstWeekday` from the system.
- `includesToday = tokens(today) > 0`.
- Count backwards starting from today if it is active, otherwise from yesterday, while each day is active.
- Days step by calendar day, never by 86400 s.

**Streak badge** (`UI/StreakBadge.swift`):
- Shown when `days >= 2` (`minimumDays = 2`).
- Content is the SF Symbol `flame.fill` and the day number, with 2 pt spacing and font `.caption.weight(.medium)`.
- Flame color is `Color.orange` if `includesToday`, else `Color.secondary`. The number is `.secondary` with monospaced digits.
- Tooltip and accessibility label:
  - If today counts: `streakDays(n)`, for example "4-day streak" / "4일 연속 사용" / "4日連続".
  - Otherwise: `streakDaysPending(n)`, for example "4-day streak — use it today to keep it going" / "4일 연속 — 오늘 사용하면 이어집니다" / "4日連続 — 今日使えば続きます".

**`RecapPeriod`** (`:114-143`):
- Calendar intervals: `.weekOfYear`, `.month` or `.year`, on the Gregorian calendar with the local TZ and `firstWeekday = Calendar.current.firstWeekday`.
- `shifted(by:)` adds the component to `start` and re-derives the interval.

**`UsageRecap.make`** (`:190-268`):
- `elapsed` = the period's days that are `<= today`, as `(key, tokens)`.
- `total = Σ elapsed.tokens`.
- `bestStreak` = the longest run of `tokens > 0` within `elapsed`.
- `activeDays` = the count of elapsed days with `tokens > 0`.
- `countedDays` = the count of elapsed days that are `known`, where `known(k) = tokens(k) > 0 || ledger.covers(k)`.
- `bestDay` = the elapsed day with the maximum tokens among `> 0`. On ties, Swift `max(by:)` returns the **last** maximal element.

Buckets:
- **Week and month.** One bucket per day of the whole period, future days included. `isCurrent = date == today`. `hasData = date <= today && known(key)`.
- **Year.** 12 buckets. Each key is the first day of its month. Tokens are the sum over days `<= today` in that month. `isCurrent` means today falls in that month. `hasData` means any elapsed day in the month is known.

Comparison with the previous period:
- `isInProgress = start <= today < end`.
- The previous period is `shifted(-1)`. Its day keys are `previousDays`.
- `compared` = the first `elapsed.count` previous days when in progress, otherwise all previous days.
- `previousTotal = Σ compared` **only if** `ledger.covers(compared.first)`, else `nil`.
- `delta = (total - prev) / prev`. It is nil if `prev` is nil or 0.

Other outputs:
- `canGoBack = ledger.covers(previousDays.last)`.
- `graduated` = dex entries with `!isReleased` and `caughtAt ∈ [start, end)`, sorted newest first. The view shows at most 4.

### 3.5 Daily trend bars (`UI/PopoverView.swift:1170-1784`)

**Data**:
- **`DailyTrendWindow`** (`Models.swift:854-870`) has the raw values `"7d"`, `"14d"`, `"21d"` and `"month"`. Day counts are 7, 14, 21 and nil.
- The UserDefaults key is **`dailyTrendWindow`**. The default is **`.d14`** (`UsageStore.swift:953`), and unknown raw values fall back to `.d14`.

**Store series** (`UsageStore.swift:539-634`):
- `monthDailyTotals` is a union-by-date of each snapshot's `monthDaily`. Tokens are summed for all five fields (input, output, cache write, cache read, total). Cost and coverage are summed only for `reportsCost` snapshots. The result is sorted by date string.
- `rollingDailyTotals(days)` works the same way on `snapshot.recentDaily ?? snapshot.monthDaily`, taking `.suffix(days)` per snapshot.
- `trendProviders(window)` gives one `ProviderSeries{id, name, days, reportsCost}` per snapshot that has a series. For rolling windows it uses `recentDaily.suffix(n)`, falling back to `monthDaily.suffix(n)`.
- `hasDailyTrendHistory`: any snapshot has a `monthDaily` or `recentDaily` day with `totalTokens > 0`.

**Provider series** (`UsageProvider.swift:50-81`):
- `monthDaily` runs from the 1st to today with explicit zero days.
- `recentDaily` covers the last **21** days (`maxDailyTrendWindowDays = 21`, `LocalUsageReader.swift:2145`) up to today, with explicit zeros.
- `weekTotal` covers `startOfWeek(now)` (system `firstWeekday`) to today.
- `monthTotal` covers the 1st of the month to today.

**Columns** (`:1511-1560`):
- **Month.** One column per day 1…`daysInMonth` (computed on the Gregorian calendar in UTC) of `today`'s month. `tokens` comes from the series or is 0. `isFuture = day > todayOfMonth`. Days from other months never appear.
- **Rolling N.** N columns ending at today, built with calendar-day arithmetic. `isFuture` is always false.

**Display gate** (`:1221-1246`):
- `peak = max(columns.tokens)`. If `peak > 0`, draw the full chart.
- Otherwise, if `window != nil && hasAvailableHistory`, draw only the caption row so the window menu stays reachable.
- Otherwise draw nothing.

**Geometry** (`DailyTrendMetrics`, `:1490-1510`):

| constant | value |
|---|---|
| `track` | 26 pt |
| `spacing` | 1.5 pt |
| `baseline` | 1.5 pt |
| `tickHeight` | 1.5 pt |

`barHeight` is `baseline` if `peak <= 0` or `tokens <= 0`. Otherwise it is `max(baseline, min(1, tokens/peak) * 26)`. Columns have equal width, each `maxWidth: .infinity`.

**Peak**: shown in the caption only when **more than one** column has `tokens > 0` (`showsPeak`, `:1567`). The label is `l.peakDay`, for example "Peak" / "최다" / "最多".

**Single-provider coloring** (`:1424-1436` and the barRow):

| bar | fill |
|---|---|
| today | `Color.accentColor` |
| hovered | `secondary @ 0.8` |
| empty day (0 tokens) | `secondary @ 0.18` |
| other days | `secondary @ 0.45` |

The bar shape is a `RoundedRectangle(cornerRadius: 1, .continuous)`. Future columns are clear, have no hover and no tick.

**Stacking** (`DailyTrendStack`, `:1713-1784`):
- `ordered`: providers with month total `> 0`, sorted by total descending, with ties broken by id ascending. The largest provider sits at the bottom.
- Stacking happens when `ordered.count > 1`.
- Segments for a date: providers with tokens `> 0` that day, bottom to top in order. Heights are cumulative: `top = barHeight * running/total`, and the last segment uses the exact `barHeight`. Each segment height is `top - placed`.
- Opacity is 1 when the day is today or hovered, 0.55 otherwise.
- The whole bar is clipped with corner radius 1.
- Empty days in stacked mode use the single-color baseline bar.

**Provider colors** (`:1202-1219`): the slot is `registeredProviderIDs.firstIndex(of: id) % 16`. An unknown id uses slot 0. The registration order (`UsageStore.swift:893-899`) is:

```
claude_code, codex, gemini, antigravity, opencode, hermes, cursor, grok, copilot, kiro, pi, omp, omo, aside, kimi_code, devin
```

The palette is the same in light and dark:

| slot | color | slot | color |
|---|---|---|---|
| 0 | #D97857 | 8 | #E0CC99 |
| 1 | #5C9EF2 | 9 | #B2D161 |
| 2 | #73C785 | 10 | #996B4C |
| 3 | #C78CEB | 11 | #4C998C |
| 4 | #F2C252 | 12 | #CC5966 |
| 5 | #59C7CC | 13 | #667ACC |
| 6 | #ED82AD | 14 | #F59438 |
| 7 | #9EA3B2 | 15 | #C29461 |

Claude Code is #D97857 and Codex is #5C9EF2.

**Weekend tick row**: a 1.5 pt bar under every non-future column where `Calendar.current.isDateInWeekend(date)`. Its color is `secondary @ 0.5`.

**Axis labels** (`axisLabel`, `:1590-1646`). `minimumSeparation` is 3 and `labelInterval` is 7.
- `date == today`: always label it with the day-of-month (no leading zero), colored with the accent color.
- **Month window.** Label day 1 and days divisible by 7. Suppress a label if `|day - todayOfMonth| < 3`.
- **7-day window.** `diff = days from date to today`, and only `diff > 0` is considered.
  - Day 1 is labelled iff `diff >= 3 && (diff == 3 || |diff-3| >= 3)`.
  - Otherwise the label is placed only where `diff == 3`.
- **14 and 21-day windows.** Day 1 is labelled iff `diff >= 3`. A column with `diff % 7 == 0 && diff >= 3` is labelled unless one of these holds:
  - `(diff - (day-1)) >= 3 && (day-1) < 3`: it would sit near a visible 1st.
  - With `daysToNextFirst = daysInMonth - day + 1`: `diff - daysToNextFirst >= 3 && daysToNextFirst < 3`: it would sit near the next month's 1st.
- Label colors: today uses the accent color, future columns `.tertiary`, everything else `.secondary`. Font is `.caption2` with monospaced digits. Unlabelled columns keep their width.

**Caption row** (`:1257-1303`), an `HStack` with spacing 5:
1. Window title. When selectable, it is a borderless `Menu` with items for 7, 14, 21 days and this month, and a checkmark on the active one. Font is `.caption` in `.tertiary`.
   - The titles are `dailyTrend7d`, `dailyTrend14d`, `dailyTrend21d` and `dailyTrendMonth`:

     | window | en | ko | ja |
     |---|---|---|---|
     | 7d | Last 7 days | 최근 7일 | 直近7日 |
     | 14d | Last 14 days | 최근 14일 | 直近14日 |
     | 21d | Last 21 days | 최근 21일 | 直近21日 |
     | month | This month | 이번 달 | 今月 |
2. The readout: **today's** combined values, which do not follow hover. Font is `.caption.weight(.medium)` in `.secondary` with monospaced digits.
3. A spacer.
4. If `showsPeak`: the "Peak" label (`.caption2` `.tertiary`) followed by the compact peak (`.caption2.semibold` `.secondary`).
5. The page dots.

**Readout** (`:1305-1313`):
- With cost: `"\(dayStamp) \(compact) \(cost.text)"`, for example `Thu, 11/5 1.1M $2.76`.
- Without cost: `"\(dayStamp) \(compact)"`.
- Empty if today is not in the series.

**`dayStamp`** (`:1663-1671`) uses the locale template `"MdE"` with the **app language's** locale:

| locale | example |
|---|---|
| en | `Thu, 11/5` |
| ko | `11. 5. (목)` |
| ja | `11/5(木)` |
| fr | `jeu. 05/11` |

**Hover tooltip** (`:1337-1382`, `DailyTrendHover:1673-1711`):
- Appears on the hovered non-future column, with no delay.
- The card floats **4 pt above** the bar row and is centered on the column. The column center is `(width + 1.5)/count * (index + 0.5)`. The card is clamped to `[0, width - cardWidth]`. It is drawn over the content above (zIndex 1) and does not take hits.
- Card content:
  - First line, `.caption` with spacing 5: the stamp (semibold), compact tokens, and cost (`.secondary`) when `showsCost`.
  - When stacked, one `.caption2` row per provider with tokens `> 0` that day, in stack order: a 6×6 swatch (radius 1), the name (`.secondary`), a spacer (min 8), compact tokens, and cost when `showsCost && provider.reportsCost`.
- Card style:
  - Monospaced digits, `lineLimit(1)`.
  - Padding: 7 pt horizontal, 5 pt vertical.
  - Background `windowBackgroundColor` (light ≈ #ECECEC, dark ≈ #323232), radius 6 continuous.
  - 0.5 pt border at `secondary @ 0.35`.
  - Shadow black @ 0.25, radius 4, y 1.

**Legend** (stacked only): a wrapping row with 8 pt item spacing and 2 pt line spacing. Each item is a 6×6 swatch, 3 pt gap, then the name in `.caption2` `.tertiary`. It sits 2 pt below the axis.

**Vertical layout**: caption, bars (26 pt), weekend ticks, axis, then legend, all in a VStack with spacing 3 and `padding(.top, 4)`.

### 3.6 Heatmap (`UI/ActivityHeatmap.swift`)

Constants:

| name | value |
|---|---|
| `weeks` | 16 |
| `cell` (row height) | 10 pt |
| `spacing` | 2 pt |
| gap between labels and grid | 4 pt (spacing + 2) |

Data:
- `days = ledger.tokensByDay`, overlaid with `max(ledger, series)` using **`monthDailyTotals`**, not the selected bar window (`:117-123`).
- `recordedSince = min(ledger.coveredSince, min(series.date))` (`:127-129`).

`recentGrid` (`:88-112`):
- `thisWeek` = the start of today's week, using `Calendar.current` and its system `firstWeekday`.
- `firstWeek = thisWeek - 15 weeks`.
- Columns run from `firstWeek` to today. Each column has 7 cells starting at `firstWeekday`.
- A cell is `isOutsideRange` if `key < first`, `key > today` (future days of the current week), or `key < max(windowStart, recordedSince ?? today)`.

Shading:
- `level` (`:135-142`) is 0 if tokens are 0. Otherwise `ratio = tokens/peak`, where `peak` = the maximum tokens among in-range cells: level 1 if `ratio <= .25`, 2 if `<= .50`, 3 if `<= .75`, else 4.
- Fill colors by level:

  | level | light | dark |
  |---|---|---|
  | 0 | #EBEDF0 | #161B22 |
  | 1 | #9BE9A8 | #0E4429 |
  | 2 | #40C463 | #006D32 |
  | 3 | #30A14E | #26A641 |
  | 4 | #216E39 | #39D353 |

  Outside-range cells are clear.
- Cell shape: `RoundedRectangle(cornerRadius: 2, .continuous)`, flexible width, fixed 10 pt height.
- Today's cell gets a 1 pt inner stroke of `primary @ 0.55`.

Display and labels:
- Gate: drawn only if `peak > 0`. There is no empty state.
- Weekday labels come from `veryShortStandaloneWeekdaySymbols` in the app-language locale, rotated to `firstWeekday`: en `S M T W T F S`, ko `일 월 화 …`. They are 8 pt system font in `.tertiary`, each 10 pt tall.

Caption, same layout as the bars:
- Title `recentHeatmap` ("16 weeks" / "최근 16주" / "16週間"), `.caption` `.tertiary`, `lineLimit(1)`.
- The readout for `hovered ?? today`, `.caption.medium` `.secondary`, `fixedSize`.
- Spacer, then Peak if `used.count > 1`, then the dots.

Readout:
- If the day is in the month series, use the bars' readout with cost.
- Otherwise `"\(dayStamp) \(compact(tokens))"`, with no cost because the ledger keeps no cost.

Hover:
- Only in-range cells set `hovered`.
- Leaving a cell clears it.

### 3.7 View switching (`UI/ActivityVisualization.swift`)

- Enum raw values `monthlyBars` (the default and first) and `recentHeatmap`. The UserDefaults key is **`activityVisualization`**, and unknown values fall back to `monthlyBars`.
- Tapping anywhere on the panel calls `next` (cyclic over `allCases`). The tooltip is `activitySwitchHint`: "Click to switch the view" / "클릭해서 다른 표시로 전환" / "クリックで表示を切り替え".
- Page dots: 4×4 circles with 3 pt spacing. The current dot is `secondary @ 0.75` and the others `@ 0.25`.
- Bars get `series = dailyTrendTotals(window)` and `providers = trendProviders(window)`. The heatmap gets `heatmapSeries = monthDailyTotals`.

### 3.8 Pace tier and limits row

**`PaceTier`** (`Core/PaceTier.swift`) has the cases `wayUnder, under, onPace, slightlyOver, over, wayOver` with raw values 0…5.
- `minimumPace = 0.1`. When `pace` is nil or `< 0.1`, `tier` is nil and the absolute thresholds are used.
- `tier`: if `utilization >= crit` it is `wayOver`. Otherwise `d = round(utilization - pace*100)`:

  | delta | tier |
  |---|---|
  | `d < -25` | wayUnder |
  | `d < -10` | under |
  | `d < 5` | onPace |
  | `d < 15` | slightlyOver |
  | `d < 30` | over |
  | otherwise | wayOver |

  Swift `.rounded()` rounds half away from zero.
- `gauge`: use `tier` if non-nil. Otherwise `>= crit` gives `wayOver`, `>= warn` gives `over`, else `onPace`.
- `isCalm` is `rawValue <= onPace`.

**Colors** (`PopoverView.swift:1949-1964`), as system colors:

| tier | color | light | dark |
|---|---|---|---|
| wayUnder | blue | #007AFF | #0A84FF |
| under | teal | #30B0C7 | #40C8E0 |
| onPace | green | #28CD41 | #32D74B |
| slightlyOver | yellow | #FFCC00 | #FFD60A |
| over | orange | #FF9500 | #FF9F0A |
| wayOver | red | #FF3B30 | #FF453A |

The percentage text uses the same color, except `slightlyOver`, which uses `.primary`.

**Thresholds**:
- The defaults are warn 80 and crit 95, with a gap of 5 (`UsageStore.swift:90-102,940-942`).
- On load, `warn = min(storedWarn, crit - 5)`.

**`paceFraction`** (`UsageStore.swift:485-490`):
- Computed as `(span - (reset - now)) / span`. The result is nil if `span <= 0`, the value is not finite, or it falls outside `[0, 1]`. It is never clamped.
- `span` is 5 h (18000 s) for session windows and 7 d (604800 s) for weekly windows.
  - Codex uses `windowDurationMins * 60`.
  - Grok uses `periodEnd - periodStart`.
  - Cursor and the Codex personal spend limit have no span, so they get no marker.

**Display mode**:
- `displayPercent` is `max(0, 100 - u)` in `remaining` mode, else `u`.
- The text is `percent(displayPercent)`. In remaining mode it is wrapped by `percentRemaining`: "62% left" / "62% 남음" / "残り62%".
- The UserDefaults key is **`limitDisplayMode`** (`used` | `remaining`), default `used`.

**`quotaRow`** (`:758-797`):
- VStack, spacing 2.
- First line, HStack: the name in `.callout`, a spacer, the optional detail (`.caption` `.secondary` with monospaced digits), then either the reset label (`.caption` `.tertiary`) or the idle hint (`.caption` `.tertiary`), then the percentage (`.callout` with monospaced digits, colored `percentColor`).
- Below it, `LimitProgressBar`.
- Tooltip, only when a pace exists:
  - With a tier: `"\(paceTier) · \(paceDelta)\n\(paceHint)"`. `paceDelta` is omitted when the rounded delta is 0.
  - With no tier (pace below 0.1): `paceHint` only.
- `paceHint` reads "Pace — an even burn across this window would sit at X now." X is `percent(displayPercent(pace*100))`.
- `paceDelta(n)`: "n pts more used" when n > 0, "n pts less used" when n < 0. ko: "페이스보다 n%p 더 씀" / "덜 씀". ja: "ペースよりnpt多く使用" / "少なく使用".
- Tier names:

  | tier | en | ko | ja |
  |---|---|---|---|
  | wayUnder | Well under pace | 여유 많음 | かなり余裕 |
  | under | Under pace | 여유 | 余裕あり |
  | onPace | On pace | 페이스대로 | ペース通り |
  | slightlyOver | Slightly fast | 조금 빠름 | やや速い |
  | over | Fast | 빠름 | 速い |
  | wayOver | Very fast | 매우 빠름 | とても速い |

**`LimitProgressBar`** (`:1899-1946`):
- A small linear `ProgressView` with value `clamp(displayPercent(used), 0, 100)` and the tint `gauge.color`.
- The track is 6 pt tall inside a 12 pt layout cell. The track background is the system unfilled track: light ≈ black @ 0.1, dark ≈ white @ 0.1.
- Pace marker:
  - A rounded rect 2.5 pt wide and 10 pt tall (track 6 + 2×2 overhang), with corner radius 1.25, filled `primary @ 0.55`.
  - It is vertically centered. `x = (width - 2.5) * markerFraction`, where `markerFraction = displayPercent(pace*100)/100`, so it mirrors in remaining mode.
  - It takes no hits and has no dedicated timer; it moves on re-render.

**Reset label** (`:705-725`):
- Format: `Text(reset, style: .relative)` followed by `" (clock)"`.
- The clock uses the template `"HHmm"` if the reset is today or within ≤ 6 h (`resetClockWindow = 21600 s`), else `"EEEEdHHmm"`. It uses the app-language locale and is always 24 h.
- Examples: "1 hr, 59 min (16:57)" and "2 days, 23 hr (Thursday 1, 14:57)".

**Forecast row** (`:1081-1101`, `UsageStore.swift:693-722`):
- Only for the tracked Claude account, placed under the 5-hour row.
- If `utilization >= 100`, the result is `(now, beforeReset: true)`.
- Otherwise the inputs need `utilization ∈ [5, 100)`, `blockTokens > 0` and `tokensPerMinute >= 10_000`:
  - `tokensPerPercent = blockTokens / utilization`
  - `minutesLeft = (100 - u) * tokensPerPercent / tpm`
  - `minutesLeft` must be finite and `< 1440`.
  - `depletion = now + minutesLeft`
  - `beforeReset = depletion < reset`
- Display: an icon (`exclamationmark.triangle.fill` or `checkmark.circle`, `.caption2`) and text in `.caption`.
  - When it hits before reset: `forecastReach("HH:mm")`, "At current rate, limit hit at 14:05", colored orange.
  - Otherwise: `forecastNoReach`, "Won't hit limit before reset at current rate", in `.tertiary`.
  - Leading padding 2.

**Stale badge**:
- Text `"Stale · <relative>"` in `.caption`, orange.
- The Claude refresh row shows "Load official limits" (`.caption` `.secondary`) next to a small "Refresh" button. A spinner shows while refreshing, and an orange `.caption2` error line appears below.

**Section title**: `limitsOfficial`, "Limits (official)" / "한도 (공식)" / "上限（公式）", in `.caption` `.secondary`. Rows are spaced 8 pt in a VStack. An expired account's rows render at `opacity 0.5`.

**Claude row order**:
1. 5-hour session (with the idle hint "Starts with your next message" when `utilization == 0 && reset == nil`)
2. Forecast
3. Weekly
4. Weekly Opus
5. Weekly Sonnet
6. `scopedLimitEntries`
7. "Claude current 5h block" with tokens and "Reset <relative> (clock)", in `.caption`

### 3.9 Footer "Updated"

- `lastUpdated` is set at the end of phase 1 when there are no errors. When there are errors, it is set only if it was nil and snapshots exist (`UsageStore.swift:1192-1198`).
- Text: `l.updated + " " + Text(date, style: .relative)`, for example "Updated 0 sec", "Updated 1 min, 5 sec", "Updated 2 hr, 3 min". It is `.caption` `.tertiary`. SwiftUI re-renders it every second.
  - Style: the largest two nonzero units, abbreviated, comma-separated.
  - Labels: ko "갱신 0초", ja "更新 0秒".
- The orange `exclamationmark.triangle` shows when `lastErrorDescription != nil`. Its tooltip is `lastErrorMessage(l)`.
- The footer items for refresh, Updated and error show only on the Home tab.

## 4. Data and persistence

| key (UserDefaults, macOS) | type | default | notes |
|---|---|---|---|
| `dailyTokenLedger` | `{yyyy-MM-dd: Int}` | `{}` | recap, streak and heatmap history |
| `dailyTokenLedgerSince` | String? | nil | first covered day |
| `dailyTrendWindow` | `"7d"`, `"14d"`, `"21d"` or `"month"` | `"14d"` | |
| `activityVisualization` | `"monthlyBars"` or `"recentHeatmap"` | `monthlyBars` | |
| `limitDisplayMode` | `"used"` or `"remaining"` | `used` | |
| `warnThreshold` / `critThreshold` | Double | 80 / 95 | gap 5 |
| `refreshInterval` | seconds | 120 | 0 = manual |

`nav.providerID` and `nav.claudeAccountID` are in-memory only. They are kept across popover openings but not persisted.

**`ProviderSnapshot`** (`Models.swift:832-852`) carries:
- `providerID` and `displayName`.
- `today: DailyUsage?`, whose fields are `date`, `inputTokens`, `outputTokens`, `cacheCreationTokens`, `cacheReadTokens`, `totalTokens`, `totalCost`, `costCoverage` and `models: {model: totalTokens}?`.
- `activeBlock`, `weekTotal` and `monthTotal`.
- `monthDaily` and `recentDaily`.
- `fetchedAt`, `reportsCost` and `lastUsage`.

**Refresh pipeline** (`UsageStore.swift:1095-1309`):

Phase 1 fetches today per provider:
- On fetch failure, keep the previous `today` only if it has today's date.
- On success with no data, `today = nil`.
- Snapshots are kept only if `hasDisplayableUsage` holds (`:2306-2314`), meaning at least one of:
  - `today` is non-nil;
  - any block, week, month, monthDaily or recentDaily value has tokens `> 0`;
  - `lastUsage` is within 7 days.

Phase 2 runs enrichment:
- It replaces block, week, month, monthDaily, recentDaily and lastUsage only when `periodsOK`.
- It drops snapshots that no longer have displayable usage.
- It sorts snapshots by provider registration order.
- It then records the ledger.

Midnight (`NSCalendarDayChanged`), wake and network reconnect each trigger a refresh. A refresh that arrives during a running one is coalesced into one re-run. If the first result is empty with no errors, one retry runs after 20 s.

**Per-model list** (`PopoverView.swift:345-361`):
- Shown when `today.models.count > 1`. `models` is filled only by providers that call `daily(..., includeModels: true)`: **pi, omp, omo, kimi_code** (`LocalUsageProvider.swift:118,145,171,198`). Claude Code and Codex **never** show it.
- Rows are sorted by tokens descending.
- The name is the last `/` component, in `.caption2` `.secondary`, `lineLimit(1)` with middle truncation. The value is compact, in `.caption2` `.tertiary` with monospaced digits.

## 5. UI spec

### 5.1 Typography and colors (macOS)

macOS fonts are SF Pro and SF Mono:

| style | size |
|---|---|
| `.caption` | 10 pt |
| `.caption2` | 10 pt |
| `.callout` | 12 pt |
| body | 13 pt |
| big total | 28 pt bold, monospaced digits |

On Windows use Segoe UI Variable and Cascadia Mono, or `ui-monospace`. Keep the pt values as CSS px. The Windows popover is 360 px wide, so 1 pt maps to 1 CSS px.

Label colors:

| role | light | dark |
|---|---|---|
| `.primary` | #000000 @ 0.85 | #FFFFFF @ 0.85 |
| `.secondary` | #000000 @ 0.50 | #FFFFFF @ 0.55 |
| `.tertiary` | #000000 @ 0.26 | #FFFFFF @ 0.25 |
| accent (default blue) | #007AFF | #0A84FF |
| divider | #000000 @ 0.10 | #FFFFFF @ 0.10 |

The popover background in dark appears to be about #2A2A2A to #2B2B2B in the screenshots. Light is about #ECECEC.

### 5.2 Component tree (Home)

```
Popover 360w, padding 14, VStack(spacing 12)
├─ updateBanner?
├─ Segmented tabs [Home | Shop | Bag | Collection]
├─ ScrollView (height 520, trailing inset 12)
│   └─ VStack(spacing 12)
│       ├─ CompanionHeader (sprite 76×76 radius 12, bg secondary@0.06; name callout semibold; LabeledProgressBar 6→12 on hover 300 ms)
│       ├─ Divider
│       ├─ UsageHeader VStack(leading, spacing 6)
│       │   ├─ "Today's tokens" caption secondary
│       │   ├─ HStack(firstTextBaseline): compact 28 bold │ grouped caption secondary (if ≥1000) │ Spacer │ cost callout secondary (if showsCost)
│       │   ├─ ViewThatFits(horizontal), padding top 2:
│       │   │   A: HStack(14): periodTotals │ Spacer(0) │ StreakBadge │ RecapButton
│       │   │   B: VStack(trailing, 6): periodTotals(full width, leading) ; HStack(14){StreakBadge, RecapButton}
│       │   │   periodTotals = ViewThatFits{ HStack(14){week, month} | VStack(leading,4){week, month} }
│       │   │   periodLabel = HStack(4): name caption tertiary │ compact caption semibold mono │ cost caption secondary
│       │   ├─ ActivityPanel (bars | heatmap), whole area tappable
│       │   ├─ ProviderTabBar (if snapshots.count > 1), padding top 6
│       │   └─ ProviderRow (if selected snapshot has today)
│       ├─ Divider
│       ├─ ProviderStatusBanner? (7×7 dot + label caption medium + description caption secondary)
│       └─ LimitsSection? + Divider
└─ Footer HStack(spacing 10)
```

**RecapButton**:
- Content: an HStack with spacing 3 of SF Symbol `chart.bar.xaxis` and `recapDetails` ("Details" / "상세" / "詳細") in `.caption`.
- Padding: 8 pt horizontal, 4 pt vertical.
- Background: `primary @ 0.08` in a Capsule.
- Plain button style. Tooltip and accessibility label: `recapOpen` ("Usage recap" / "사용량 돌아보기" / "使用量のふりかえり").

**ProviderTabBar / CapsuleTabBar** (`:1845-1890`):
- A horizontal `ScrollView` with no indicators. It bounces only when the content overflows.
- An HStack with spacing 6 holds the capsules. Each capsule:
  - Text `lineLimit(1)` and `fixedSize`.
  - Font `.caption`, semibold when selected.
  - Padding: 10 pt horizontal, 4 pt vertical.
  - Background `accent @ 0.18` when selected, else `secondary @ 0.08`.
  - Foreground accent when selected, else `.secondary`.
- Selection resolves the preferred id, falling back to the first snapshot.

**ProviderRow** (`:320-363`), VStack(leading, spacing 3), `padding(.top, 2)`:
- First line, HStack: the display name (`.callout.semibold`), a spacer, compact today (`.callout` with monospaced digits), and the cost (`.caption` `.secondary`) if `reportsCost`.
- Token rows, VStack spacing 3:
  - `HStack(10){Input, Output}`
  - `HStack(10){Cache write, Cache read}`
  - Each label is `HStack(3)`: the name in `.caption2` `.tertiary`, then the compact value in `.caption2` `.secondary` with monospaced digits.
  - Labels: en "Input/Output/Cache write/Cache read", ko "입력/출력/캐시 쓰기/캐시 읽기", ja "入力/出力/キャッシュ書込/キャッシュ読込".
- Then the per-model rows (section 4).

**Footer** (`:1105-1166`):
- On Home only: `arrow.clockwise` (borderless, tooltip "Refresh now"), "Updated <relative>" (`.caption` `.tertiary`), and the error triangle.
- Spacer.
- `eye` or `eye.slash`: toggles `floatingPetEnabled`. Tooltip is "Hide pet" or "Show pet".
- `gearshape`: opens Settings.
- `power`: quits.

### 5.3 Recap screen (`UI/UsageRecapView.swift`)

**Top row**, HStack spacing 6:
- "‹ Back" (`chevron.left`, borderless). Esc also goes back.
- Spacer(4).
- A small segmented picker with Week / Month / Year ("주간/월간/연간", "週/月/年"). The default is Week. Changing scope resets `offset = 0`.

**Navigation row** (`.caption`, borderless):
- "◀ Previous" (`arrowtriangle.left.fill`), disabled unless `canGoBack`.
- The current-period button: "This week", "This month" or "This year". Disabled when `offset == 0`.
- "Next ▶", disabled when `offset >= 0`.
- `offset` is never positive.

**RecapCard**: 332 × 226 pt (`contentWidth = 360 - 28`), padding 12, VStack spacing 8. The card uses a forced light color scheme and fixed colors.

Shell:
- Linear gradient from top #D6362E to bottom #A81F1C.
- A 5 pt black @ 0.14 strip on the left edge.
- Clipped to corner radius 10.

Header, HStack spacing 6, leading padding 2:
- Lens: 16 pt circle with a radial gradient from #B8E0FF to #2673D9. The gradient center is (0.35, 0.3) and its end radius is 11. It has a 1.5 pt stroke of white @ 0.9 and a 4 pt white @ 0.75 highlight offset (-3, -3).
- Three 6 pt dots: #F25959, #FAD159 and #73E080, each with a 0.5 pt black @ 0.25 stroke.
- Spacer(4).
- The period label, uppercased. Font is SF Mono 10 bold with tracking 1.2, in white @ 0.95, with minimum scale 0.7.
  - Week: date-interval template `"dMMMy"`, for example "OCT 11 – 17, 2026".
  - Month: template `"MMMM y"`.
  - Year: "2026".

Screen:
- Background is #0D1214 with corner radius 6. A radial overlay of LCD @ 0.07 sits at (0.15, 0.1) with radius 180. There is a 2 pt black @ 0.35 inner stroke. Padding is 10 and horizontal padding 2.
- Content is a VStack with spacing 7.
- LCD color is #82FA9E. Amber is #FFBA47.

Total line (first text baseline, spacing 5):
- `compact(total)` in mono 24 bold LCD.
- `recapTokensUnit` uppercased ("TOKENS"), in mono 7 medium with tracking 0.6, at LCD @ 0.5.
- Spacer(4).
- The delta chip, only if `delta != nil`:
  - Text: `"▲"` if `delta >= 0` else `"▼"`, followed by `percent(|delta|*100)`.
  - Mono 9 bold. Colored LCD when up and amber when down.
  - Padding: 5 pt horizontal, 2 pt vertical.
  - Border: 1 pt at color @ 0.6, radius 2.

Meters:
- An HStack aligned to the bottom. Spacing is 6 for week, 1.5 for month and 4 for year.
- Each bucket is a VStack with spacing 3: the meter, then a label in a zero-width, 10 pt tall frame so the label does not widen the column.
- Meter: 8 blocks, each 4 pt tall with 2 pt gaps.
  - `lit = tokens > 0 ? max(1, round(tokens/peak*8)) : 0`, where `peak = max(1, max bucket tokens)`.
  - Lit blocks fill from the bottom. They are LCD when the bucket is current, else LCD @ 0.55.
  - Unlit blocks are LCD @ 0.09 when `hasData`, else @ 0.035.
- Labels:
  - Week: `shortStandaloneWeekdaySymbols`, first 2 letters, uppercased (SU MO …).
  - Month: the day number only on days 1, 8, 15, 22 and 29.
  - Year: `veryShortStandaloneMonthSymbols`, uppercased.
  - Font is mono 8 for week and 7 otherwise. The current bucket is bold LCD; the others are regular at LCD @ 0.45.
- Tooltip and accessibility label: `"<EEEE d MMM>, <grouped tokens>"`. For the year scope the name is `"MMMM y"`. Buckets without data say "no data".

Readouts, HStack spacing 10 with two columns, each a VStack with spacing 3:
- Column 1: Best day (`compact` or `"--"`) and Best streak (`recapDays(n)`, "6d" / "6일" / "6日").
- Column 2: Active days (`"active/counted"`) and Graduated (count).
- Labels are mono 7 medium, uppercased, at LCD @ 0.5. Values are mono 9.5 bold LCD. Both have minimum scale 0.7.

Graduate strip, HStack spacing 5, leading padding 2:
- The current companion sprite at 18 pt, static.
- A 1×14 white @ 0.35 divider.
- If there are no graduates: `recapNoGraduates` in mono 8, white @ 0.8, up to 2 lines.
- Otherwise up to 4 chips. Each chip:
  - An HStack with spacing 2: an 18 pt sprite with ✨ (7 pt, offset (3, -2)) when shiny, then the name in mono 8 semibold white with minimum scale 0.7.
  - Padding: 4 pt horizontal, 2 pt vertical. Background black @ 0.22, radius 3.

**Below the card** (`.caption` `.secondary`, VStack spacing 2):
- When there is a best day: `recapBestDayLine(dayLabel "EEEE d MMM", grouped tokens)`, for example "Your best day was Sunday, Oct 11 — 910,000 tokens."
- When in progress with a delta: `recapCompareSoFar(scope)`, for example "Compared with the same days last week".

### 5.4 States

**Empty states**:
- With no snapshots, today shows "0". There is no provider tab and no provider row.
- The bars are hidden unless history exists.
- The heatmap is hidden when `peak == 0`.
- The streak badge is hidden below 2 days.

**Before the first refresh**: `lastUpdated` is nil, so there is no "Updated" text. The menu bar shows "—".

**Loading**: there is no spinner for usage. The Claude limits refresh button shows a small `ProgressView`.

**Error**: the footer shows a triangle with a tooltip. Values are kept from the previous snapshot.

**Hover**:
- Bars: the tooltip card appears immediately and the bar brightens.
- Heatmap: the caption readout follows the hovered cell.
- Limits row: the native tooltip after the system delay.
- Recap meters: the native tooltip.

## 6. Windows adaptation

| macOS mechanism | Windows / Tauri equivalent |
|---|---|
| `NSStatusItem` title text (`menuLines`, compact tokens, `costCompact`, limit %) | Windows tray icons cannot show text. Put the same strings in the tray tooltip (`tray.set_tooltip`, which `lib.rs:166-184` already does). Optionally render the number into the tray icon bitmap, though 16×16 fits 3-4 glyphs at most. Keep `menuLines` joined with " · ". |
| `NSPopover` with `.transient` | Undecorated always-on-top `WebviewWindow` "main" (already 360×680 in `tauri.conf.json`), shown near the tray rect and hidden on blur. Reset `nav` (tab = Home, recap closed) on every show. Keep the provider and account tab selection in memory across shows. |
| UserDefaults keys (section 4) | `settings.json` in `app_data_dir`: add `daily_trend_window`, `activity_visualization`, and keep `limit_display`, `warn_threshold` and `crit_threshold`. Put the ledger in a separate `usage-ledger.json` with `{tokens_by_day, covered_since}` so settings writes cannot clobber it. |
| `Locale.current` grouping (`grouped`) | `new Intl.NumberFormat(undefined)` or `navigator.language` (WebView2 follows the Windows display language). Do **not** use the app language. |
| App-language date templates (`MdE`, `dMMMy`, `EEEE d MMM`, `MMMM y`, `HHmm`, `EEEEdHHmm`) | `Intl.DateTimeFormat(appLangLocale, {...})`: `{month:'numeric',day:'numeric',weekday:'short'}`, `formatRange` with `{day:'numeric',month:'short',year:'numeric'}`, `{weekday:'long',day:'numeric',month:'short'}`, `{month:'long',year:'numeric'}`, and `{hour:'2-digit',minute:'2-digit',hourCycle:'h23'}`. |
| `Calendar.current.firstWeekday` and `isDateInWeekend` | `new Intl.Locale(navigator.language).getWeekInfo()` (`firstDay` 1 = Mon … 7 = Sun, `weekend: number[]`). This is available in WebView2/Chromium ≥ 130; older versions use the `weekInfo` property, so read both. Rust needs the same week start for `weekTotal`: pass it from the UI into settings, or read `GetLocaleInfoEx(LOCALE_IFIRSTDAYOFWEEK)` (0 = Mon) through the `windows` crate. |
| `Text(date, style: .relative)` live ticking | A 1 s `setInterval` in the footer and limit rows. Format with the two largest nonzero units, abbreviated as `sec`, `min`, `hr` and `days`, joined by ", ", with per-language unit strings (ko 초/분/시간/일, ja 秒/分/時間/日). `Intl.DurationFormat` with style "short" is acceptable where available. |
| `.help()` tooltips | `title` attribute for static text. Use a custom hover card for the bar tooltip, which has no delay. |
| SF Symbols (`flame.fill`, `chart.bar.xaxis`, `arrow.clockwise`, `exclamationmark.triangle`, `checkmark.circle`, `eye`/`eye.slash`, `gearshape`, `power`, `chevron.left`, `arrowtriangle.left/right.fill`) | Inline SVGs (Fluent UI System Icons or Lucide equivalents: Flame, BarChart3, RotateCw, AlertTriangle, CheckCircle, Eye/EyeOff, Settings, Power, ChevronLeft, Play, mirrored). |
| `NSColor` dynamic light/dark | CSS custom properties under `@media (prefers-color-scheme: dark)`. `App.css` already follows this pattern. Use the hex values from section 5.1 and the tier colors. |
| `NSCalendarDayChanged`, `didWake`, network reconnect | Rust scheduler: compare `Local::now().date_naive()` against the last refresh day every tick, and refresh on change. For resume, use `WM_POWERBROADCAST` / `PBT_APMRESUMEAUTOMATIC` (`windows` crate) or detect a wall-clock jump of more than 2× the interval. |
| Keychain (Claude OAuth for limits) | Already `~/.claude/.credentials.json` on Windows (`limits.rs:56-82`). No Keychain is needed. |
| `UNUserNotification` limit alerts | `tauri-plugin-notification`, already used (`lib.rs:138`). |
| URL schemes, LaunchAgent | Not used by this area. |

## 7. Gaps vs the current Windows code

The current state is described by `src-tauri/src/usage.rs` (today-only Claude/Codex token sums), `src/App.tsx:129-148` (a Home card showing the today total and two provider lines) and `src/shared.ts:65` (`Intl.NumberFormat` compact).

1. **Cost.** Nothing exists: no `ModelPricing` table, no per-entry estimate, no `cost-state` parsing, no `CostCoverage`, no "Unavailable" rendering. Claude entries also do not keep `message.model`, and Codex entries do not track `turn_context.model`.
2. **Number format.** `fmt` uses `Intl` compact notation:
   - It localizes ("1.1만" in ko, "110万" in ja).
   - It rounds differently, with no 999_950 promotion rule and no 2-decimal B.
   - It must be replaced by an exact port of `TokenFormatter.compact`.
   - `usage::compact` in Rust uses `{:.1}` without trimming, so it gives "1.0K" where the source gives "1K", and "1.2B" where the source gives "1.24B".
   - `percent`, `cost` and `costCompact` are missing.
3. **Grouped exact number** next to the big total is missing. The big 28 pt total layout is also missing.
4. **Week and month totals** are missing. They need a week-start-aware scan from `min(startOfMonth, startOfWeek, now - 5 h, now - 7 d, startOfRollingWindow(21))`.
5. **Daily series.** `monthDaily` and `recentDaily` (21 days) per provider are missing, and so is the multi-day aggregation (`monthDailyTotals`, `rollingDailyTotals`, `trendProviders`). The Rust `TodayUsage { claude, codex, total }` shape must become a list of provider snapshots: `[{provider_id, display_name, today, week_total, month_total, month_daily, recent_daily, last_usage, reports_cost}]`.
6. **Daily bars chart** is entirely missing: window menu, stacking, palette, weekend ticks, axis label rules, hover tooltip, legend and peak.
7. **Heatmap** is missing, along with the 16-week grid, weekday labels and levels.
8. **View cycling** is missing, including page dots, the `activity_visualization` setting and the `daily_trend_window` setting.
9. **Day ledger** is missing: persistence, merge with max, prune to last year, coverage.
10. **Streak badge** is missing.
11. **Recap screen** is missing: all scopes, the card and navigation. The dex entries carry `caught_at` (`shared.ts:30`), but there is no `is_released` field. Confirm the graduation semantics with the companion area.
12. **Provider tabs.** The current UI lists Claude Code and Codex as two fixed rows. Capsule tabs, the selected-provider detail and the input/output/cache breakdown labels are missing. `Tokens` already has the four fields.
13. **Per-model breakdown.** It is not needed for Claude/Codex parity, because v2.5.7 shows it only for pi, omp, omo and kimi_code. It becomes needed if those providers are ported.
14. **Limits row.**
    - The current row shows "used 62%" with `Math.round`. It should match the source:
      - The name on the left.
      - Relative reset with the clock suffix on the right.
      - The percentage via `percent()`, followed by "left" in remaining mode.
    - The colors are a fixed 80/95 green/amber/red, not configurable, and are not wired to the `warn_threshold`/`crit_threshold` settings.
    - Missing: pace tiers, the pace marker, the pace tooltip, the 0.1 hold, forecast, the stale badge, `hasNotStarted` idle hint, and the "Claude current 5h block" row.
    - `resetIn` produces "2d 3h" style text without the clock suffix.
    - Codex has no `windowSpan` because only the key is kept. The window minutes must be exposed so the span is available for the pace.
15. **Limits section scope.** The source shows only the selected provider's limits, under its tab. The current code shows all providers stacked, with their names.
16. **Footer.** "Updated N sec" (live) and the error triangle are missing. A refresh button exists in the tab bar instead. `lastUpdated` and `lastError` are not emitted from Rust.
17. **Snapshot visibility rule** (`hasDisplayableUsage`, including the 7-day `lastUsage` window) and refresh coalescing are missing. The midnight refresh trigger is also missing; the scheduler only runs on its interval.
18. **Codex dedup.** `usage.rs:161` notes that fork/subagent replay dedup is not ported. This affects every total and chart. It is tracked in another area, but chart parity depends on it.
19. **Languages.** Windows has ko, en and ja only. The source has 8 (ko, en, ja, es, fr, pt, de, ru). Strings for this area need at least ko, en and ja, taken verbatim from `Localization.swift:29-330`.
20. **Number grouping locale.** No code exists yet. Use the system locale.

## 8. Open questions

1. **Tray text.** Windows cannot show text next to the tray icon. Should the compact token number be drawn into the icon bitmap, or should the tooltip suffice? The tooltip already exists.
2. **Week start for `weekTotal` on Windows.** Use the Windows regional `LOCALE_IFIRSTDAYOFWEEK`, as macOS does with `Calendar.current`, or the language's CLDR default? The recommendation is the Windows regional setting, read in Rust so the backend and UI agree.
3. **Ledger seeding.** macOS builds `coveredSince` from the first month series it sees. On a fresh Windows install, should the ledger be backfilled from all available logs (up to last Jan 1) so the recap and heatmap are not empty? macOS does not do this, so strict parity means no.
4. **Screenshot discrepancies.** Code wins in each case:
   - `screenshot-model-breakdown.png` shows an older provider row ("in / out / cache w / cache r", with monospaced model rows).
   - `screenshot-pace-marker.png` shows a gray gauge fill. Current code tints by tier.
   - `screenshot-quota-alignment.png` shows an uppercase "LIMITS (OFFICIAL)" and "1h 10m" reset style. Current code shows "Limits (official)" and the relative style "1 hr, 10 min (14:30)".
5. **Providers beyond Claude/Codex** (gemini, antigravity, opencode, cursor, grok and others) are out of scope for this area. Should their registration order still be reserved, so palette slots match macOS? The recommendation is yes: keep the full 16-id order list even if only two are implemented.
6. **Swift `String(format:"%.1f")` rounding vs JS `toFixed(1)`.** Both operate on the binary double, so they normally agree, but verify edge cases (for example 0.05, 99.95) with a small test table.
