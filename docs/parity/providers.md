# Parity spec — Usage providers (local log parsers)

Source of truth: PokeTokenBar v2.5.7 (Swift). All citations are `File.swift:line` under
`Sources/PokeTokenBar/Core/` unless prefixed with `UI/`.
Target: `src-tauri/src/usage.rs` (Rust) + `src/Settings.tsx` (React).

---

## 1. Overview

Every provider turns some on-disk store into a flat list of normalized `Entry` records
(`LocalUsageReader.swift:27-50`). All aggregation (today, week, month, daily series, 5 h block,
cost coverage, last use) is provider-agnostic and runs over that list
(`UsageProvider.swift:50-78`, `LocalUsageReader.swift:2018-2174`). A provider is therefore
"find files → parse → dedup → `[Entry]`" plus a cache strategy.

16 providers are registered, in this fixed order (it is also tab order and the Settings toggle
order) — `UsageStore.swift:894-901`:

| # | id | displayName | Store kind | Reader | Cache |
|---|---|---|---|---|---|
| 1 | `claude_code` | Claude Code | JSONL transcripts | `LocalUsageReader` | `LocalUsageCache` (disk, per-file blob, byte-offset tail) |
| 2 | `codex` | Codex | JSONL rollouts | `LocalUsageReader` | `LocalUsageCache` (parsed-rollout blob + session-id index) |
| 3 | `gemini` | Gemini | JSONL / legacy JSON | `LocalUsageReader` | `LocalUsageCache` |
| 4 | `antigravity` | Antigravity | SQLite per conversation, protobuf blobs | `LocalAntigravityUsageReader` | `LocalAntigravityUsageCache` (memory, signature-keyed) |
| 5 | `opencode` | OpenCode | SQLite + legacy JSON | `LocalAdditionalUsageReader` | `LocalAdditionalUsageCache` (memory, 30 s, time watermark) |
| 6 | `hermes` | Hermes Agent | SQLite | `LocalAdditionalUsageReader` | `LocalAdditionalUsageCache` (30 s, full re-query) |
| 7 | `cursor` | Cursor | Dashboard HTTP API → fallback SQLite `state.vscdb` | `CursorUsageAPI` + `LocalAdditionalUsageReader` | `LocalAdditionalUsageCache` (rowid watermark) + API disk cache |
| 8 | `grok` | Grok | JSONL `updates.jsonl` | `LocalUsageReader` | `LocalUsageCache` |
| 9 | `copilot` | Copilot | SQLite | `LocalAdditionalUsageReader` | `LocalAdditionalUsageCache` (id watermark) |
| 10 | `kiro` | Kiro | SQLite (pre-2.20) + JSONL (2.20+/v3), byte estimate | `LocalAdditionalUsageReader` | `LocalAdditionalUsageCache` (file signature skip + sticky merge) |
| 11 | `pi` | Pi | JSONL | `LocalUsageReader` | `LocalUsageCache` |
| 12 | `omp` | omp | JSONL (pi format) | `LocalUsageReader` | `LocalUsageCache` |
| 13 | `omo` | omo | JSONL (pi/senpi format) | `LocalUsageReader` | `LocalUsageCache` |
| 14 | `aside` | Aside | SQLite `state.db` | `LocalAsideUsageReader` | `LocalAdditionalUsageCache` (sticky merge) |
| 15 | `kimi_code` | Kimi Code | JSONL `wire.jsonl` | `LocalUsageReader` | `LocalUsageCache` |
| 16 | `devin` | Devin | SQLite | `LocalAdditionalUsageReader` | `LocalAdditionalUsageCache` (row_id watermark) |

No provider launches an external CLI for usage. `reportsCost` defaults to `true` for all 16
(`UsageProvider.swift:19-21`); cost provenance is carried per entry instead (§3.4).

---

## 2. Features (numbered)

1. **Provider protocol** — `id`, `displayName`, `reportsCost`, `fetchDaily()` (critical path:
   today only) and `fetchEnrichment()` (best effort: block/week/month/daily series/last use)
   (`UsageProvider.swift:4-39`).
2. **Shared enrichment assembly** `ProviderEnrichment.local(entries:now:)` (`UsageProvider.swift:50-78`):
   active block, week, month, month daily series, rolling 21-day series, `lastUsage`
   (= max `date` of entries with `total > 0`), `blocksOK = periodsOK = true`.
3. **Per-provider enable switch (#460)** — opt-out set persisted under `disabledProviderIDs`
   (`UsageStore.swift:243-259`). A disabled provider is not read at all; its tab is removed
   immediately; its official limits (Claude, Cursor, OpenCode Go, Grok, Antigravity, Codex) are
   cleared (`UsageStore.swift:1265, 1462, 1844, 1868, 1898, 1934, 2024`).
4. **Custom scan roots (#177)** per provider, `customScanRoots.<id>` (`CustomScanRoots.swift`),
   union with curated defaults, ancestor-eviction guard, glob per path segment, live match count
   in Settings.
5. **Shell-aware env overrides** — 19 variables resolved once per process from process env,
   then a single login-shell spawn for the missing ones (`UsageEnvironment.swift:18-82`).
6. **Incremental/append-only reading (#338)** — Claude transcripts are tail-parsed from a stored
   byte offset that always ends on `\n` (`LocalUsageReader.swift:475-534`,
   `LocalUsageCache.swift:410-429`); history.jsonl likewise (`ClaudeAccountUsage.swift:161-206`);
   SQLite stores use row-id watermarks (`LocalAdditionalUsageProvider.swift:648-770`);
   OpenCode a time watermark (`LocalAdditionalUsageProvider.swift:219-225`).
7. **Persistent per-file parse cache** `usage-cache.json` (zlib) for Claude/Codex/Gemini/Grok/
   Pi/omp/omo/Kimi with parser-version invalidation and 40-day prune (`LocalUsageCache.swift`).
8. **Codex fork/subagent replay resolution** — parent closure expansion, usage-state prefix
   matching, 1 s timing fallback, canonical ids, keep-earliest dedup (`LocalUsageReader.swift:946-1403`).
9. **Claude cost ledger** — `type:"cost-state"` per-model `costUSD` redistributed over the
   session's entries (`LocalUsageReader.swift:403-462`).
10. **Cost provenance** per entry (source / estimate / unavailable), merged into `CostCoverage`
    (`LocalUsageReader.swift:57-73`, `UsageCost.swift:5-41`).
11. **Per-model breakdown** opt-in for Pi, omp, omo, Kimi Code (`includeModels: true`).
12. **Multiple Claude config roots** — `CLAUDE_CONFIG_DIR` (comma list), `~/.config/claude`,
    detected `~/.claude-*`/`~/.claude_*` logins, Settings extras, Claude Desktop embedded
    sessions (`LocalUsageReader.swift:101-129`, `ClaudeAccountRoots.swift`).
13. **Claude per-account usage attribution** from each login's `history.jsonl`
    (`ClaudeAccountUsage.swift:20-120`).
14. **Grok subagent exclusion** via sibling `summary.json` (`LocalUsageReader.swift:1556-1571`).
15. **omp `bridge/` exclusion** (`LocalUsageReader.swift:1722-1724`).
16. **Kiro sticky merge** (deleted turns stay counted for process lifetime) and **Aside sticky merge**.
17. **Cursor dashboard API** authoritative path with disk cache and local-bubble fallback.
18. **Antigravity protobuf decoding** with step-table date recovery for 2.0 stores.
19. **Robust number parsing** — clamp to `1e18`, `null` ≠ value, no trap on `1e30`.
20. **Empty-usage retry** — when every enabled provider succeeds but nothing is displayable,
    refresh once more after 20 s (`UsageStore.swift:1311-1323`).

---

## 3. Rules & constants

### 3.1 Normalized record

```
Entry { id, date, localDay "yyyy-MM-dd" (local TZ, en_US_POSIX), model,
        input, output, cacheWrite, cacheRead,          // disjoint buckets
        explicitCost: Double?, costIsEstimate: Bool?, costUnavailable: Bool?,
        sessionID: String?, sessionIDs: [String]? }   // Claude only
total = input + output + cacheWrite + cacheRead
```
`claudeSessionIDs = sorted(unique(sessionIDs ∪ {sessionID}))` (`LocalUsageReader.swift:46-48`).

### 3.2 Number parsing (`LocalUsageReader.swift:2204-2238`)

- `maxParsedTokenValue = 1_000_000_000_000_000` (1e18).
- `doubleOrNil(v)`: `NSNumber` and not `NSNull`, finite → value; else nil. Strings are **not**
  numbers here.
- `intOrNil(v)`: nil if not a number; `<= 0 → 0`; `>= 1e18 → 1e18`; else truncate.
- `intValue(v) = intOrNil(v) ?? 0`.
- `LocalAdditionalUsageReader.intValue` (`LocalAdditionalUsageProvider.swift:1765-1779`) also
  accepts **numeric strings** (`Int(trimmed)`), clamped `[0, 1e18]`.
  `doubleValue` there accepts numeric strings too (`:1781-1785`).
- `dateValue(raw)` (`:1787-1791`): number or numeric string, finite, `> 0`;
  `raw >= 100_000_000_000` ⇒ milliseconds (÷1000) else seconds.
- Rust note: `serde_json::Value::as_u64` returns `None` for `12.0` and for negatives; implement
  `as_f64`-based clamp to match.

### 3.3 Dedup primitives

- `dedupKeepMax` (`LocalUsageReader.swift:380-397`): group by `id`; keep the entry with the
  larger `total` (ties keep the existing one); then overwrite its `date`/`localDay` with the
  **earliest** of the two; union Claude session ids (`sessionID = first`, `sessionIDs` only if >1).
  Used by every provider except Codex.
- `dedupCodexCanonicalEntries` (`:1204-1216`): keep **earliest date**, preserve first-seen order.

### 3.4 Cost (`LocalUsageReader.swift:57-73`)

Per entry, only if `total > 0`:
1. `explicitCost` finite and `>= 0` → add; coverage `.estimate` if `costIsEstimate == true`, else `.source`.
2. else if `costUnavailable != true` and `ModelPricing.estimatedCost(model, buckets)` non-nil → add, `.estimate`.
3. else `.unavailable` (and `ModelPricing.noteUnpriced(model)` logs once per model when not flagged unavailable).

`UsageCost.text`: unknown-only ⇒ "Unavailable" (`$—` compact); otherwise amount. Hint picks
partial / unavailable / estimate / reported (`UsageCost.swift:32-40`). ModelPricing itself
(table, aliases, long-context 2×/1.5× above 272 000 prompt tokens for GPT-5.4+ family and
200 000 for gemini-2.5-pro, `cacheWrite>0` with zero write rate ⇒ nil) is `ModelPricing.swift`;
port it verbatim (owned by the cost area, but every provider depends on it).

### 3.5 Windows / time constants

| Name | Value | Where |
|---|---|---|
| `blockWindow` | 5 h | `LocalUsageReader.swift:18` |
| `recentUseWindow` | 7 days | `:21` |
| `forkReplayMaximumGap` | 1 s | `:23` |
| `maxDailyTrendWindowDays` | 21 | `:2145` |
| `enrichmentScanStart` | `min(startOfMonth, startOfWeek, now-5h, now-7d, startOfRollingWindow(21))` | `:2168-2174` |
| `startOfWeek` | `Calendar.current.dateInterval(of: .weekOfYear)` → locale first weekday | `:2141` |
| `startOfRollingWindow(days)` | calendar `startOfDay(today) - (days-1) days` (DST-safe, not `days*86400`) | `:2151-2158` |
| Daily critical scan lower bound | `startOfDay(now)` (file mtime filter) | `LocalUsageProvider.swift:10` etc. |
| Claude roots TTL | 300 s | `LocalUsageReader.swift:184` |
| Embedded Claude Desktop scan depth | 7; skip `node_modules`, `.git`, `venv`, `.venv` | `:224-231` |
| Codex probe byte limit / chunk | 1 MiB / 64 KiB | `:1052-1054` |
| Codex streaming chunk | 1 MiB | `:861` |
| Filename hint usable | `len >= 4` and contains a letter/digit | `:1029-1031` |
| Additional cache freshness | 30 s | `LocalAdditionalUsageProvider.swift:205` |
| Usage cache save throttle | 60 s | `LocalUsageCache.swift:618` |
| Usage cache prune | blobs with mtime older than 40 days | `LocalUsageCache.swift:603` |
| Antigravity token ceiling | 1 000 000 000 (discard, not clamp) | `LocalAntigravityUsageReader.swift:556` |
| Antigravity timestamp plausibility | 1 000 000 000 ≤ s ≤ 4 102 444 800 | `:497, 510` |
| Antigravity named loss lines | 5 per scan | `:235` |
| Kiro bytes/token | 4 | `LocalAdditionalUsageProvider.swift:1366` |
| Cursor API page size / max pages | 100 / 200 | `CursorUsageAPI.swift:9-10` |
| Cursor API timeouts | 10 s request, 120 s deadline | `:11-12` |
| Cursor API stale cache max age | 6 h | `:13` |
| Cursor `state.vscdb` auth retries | 6, sleep `0.05*(attempt+1)` s | `LocalAdditionalUsageProvider.swift:880-891` |
| Aside busy timeout | 1000 ms | `LocalAsideUsageReader.swift:53` |
| Shell env lookup timeout | 8 s total (spawn + drain) | `BinaryLocator.swift:145` |
| BinaryLocator not-found TTL | 600 s | `BinaryLocator.swift:13` |
| Empty usage retry | 20 s, once | `UsageStore.swift:1318` |
| Settings match-count debounce | 250 ms | `UI/SettingsView.swift:992` |

### 3.6 Aggregation

- `daily(entries, localDay, includeModels)` (`:2018-2029`): sum entries with `localDay == today`;
  `nil` if bucket `total == 0`; `models[model] += total` when opted in.
- `period(from,to)` inclusive by `localDay` string compare (`:2032-2036`). Week key = `yyyy-MM-dd`
  of week start; month key `yyyy-MM` (`:2176-2180`).
- `dailySeries` (`:2061-2089`): axis built from the calendar range; zero days explicit; entries
  outside the axis dropped. Invariant: `sum(monthDaily) == monthTotal.totalTokens`.
- `activeBlock` (`:2114-2132`): entries with `date >= now-5h && total > 0`, sorted; `nil` if none;
  `minutes = max(1, (now - first)/60)`; `tpm = total/minutes`; `id = "block-<unix first>"`;
  `startTime = ISO8601(first)`, `endTime = first + 5h`, `isActive = true`.
- Snapshot is shown only if `hasDisplayableUsage` (`UsageStore.swift:2306-2314`): today non-nil,
  or block/week/month/any daily `> 0`, or `lastUsage` within 7 days.

### 3.7 Root normalization (`LocalUsageReader.swift:266-289`)

1. Resolve symlinks + standardize; dedup by lowercased path keeping first occurrence.
2. Sort by path length; drop any path equal to or under (`k + "/"`) an already-kept path —
   comparison lowercased.
3. Return the survivors in the **original** order.

### 3.8 File discovery (`LocalUsageReader.swift:358-372`, `LocalUsageCache.swift:386-406`)

Recursive enumeration with `skipsHiddenFiles` (macOS: name starts with `.` or hidden flag; the
root itself may be hidden). Extension `jsonl` only, `json` only where `allowJSON` (Gemini).
`include` predicate is evaluated **before** cache lookup. mtime filter `>= since`.

---

## 4. Per-provider specification

Notation for paths: `~` = home. Windows column: **(c)** confirmed by the tool's own
docs/convention, **(u)** uncertain — verify on a real install.

### 4.1 Claude Code (`claude_code`)

**Roots** (`LocalUsageReader.swift:101-129`), in priority order, then `normalizedRoots`:
1. For each comma-separated `CLAUDE_CONFIG_DIR` part (trim, tilde-expand): `<part>/projects`.
2. Each additional account root (`ClaudeAccountRoots.installedAccountRoots()`): `<root>/projects`.
3. `~/.config/claude/projects`. 4. `~/.claude/projects`.
5. Claude Desktop embedded: every `.claude/projects` directory found under
   `~/Library/Application Support/Claude/local-agent-mode-sessions` and `.../claude-code-sessions`
   (depth ≤ 7, skip `node_modules|.git|venv|.venv`, do not descend into a found `projects`,
   do **not** skip hidden). Log once if depth pruning happened.
6. `CustomScanRoots.union(defaults, extraRaw: customScanRoots.claude_code)`.
Cached 300 s; `invalidateProjectRootsCache()` on Settings save.

| | macOS | Windows |
|---|---|---|
| default | `~/.claude/projects` | `%USERPROFILE%\.claude\projects` (c) |
| XDG | `~/.config/claude/projects` | `%USERPROFILE%\.config\claude\projects` (u; keep — harmless) |
| env | `CLAUDE_CONFIG_DIR` (comma list) | same (c) |
| extra logins | `~/.claude-*`, `~/.claude_*` with `.claude.json.oauthAccount` | same under `%USERPROFILE%` (c) |
| Desktop | `~/Library/Application Support/Claude/{local-agent-mode-sessions,claude-code-sessions}` | `%APPDATA%\Claude\{…}` (u) |

**Files**: `*.jsonl` (no `.json`). Session id from path (`:538-544`): `<project>/<session>.jsonl`
→ `<session>`; `<project>/<session>/subagents/<agent>.jsonl` → `<session>` (subagent counts with parent).

**Line filter** (byte-level, `:498-534`): a line containing `"cost-state"` is parsed as a cost
ledger and nothing else; otherwise it must contain both `"usage"` and `"assistant"`.

**Assistant line** (`:564-580`): `type == "assistant"`, `message.usage` object, `timestamp`
ISO-8601 (fractional seconds of any width — `ISO8601Parser` truncates to 3 digits).
- `id = (message.id ?? "") + "|" + (requestId ?? "")`
- `model = message.model ?? "unknown"`
- `input = usage.input_tokens`, `output = usage.output_tokens`,
  `cacheWrite = usage.cache_creation_input_tokens`, `cacheRead = usage.cache_read_input_tokens`.
- Zero-token `<synthetic>` records are kept as entries but excluded from block/lastUsage/cost.

**Dedup**: `dedupKeepMax` within file, again across all files/roots (streaming writes the same
`(message.id, requestId)` with growing output; forks re-log with later time → keep earliest date).

**Cost ledger** (`:403-462`): line `{"type":"cost-state","modelUsage":{"<model>":{"costUSD":n}},"hasUnknownModelCost":bool}`.
Model key = text before first `[` (`claude-opus-5[1m]` → `claude-opus-5`); sum per key; ignore
non-finite / negative. Last ledger in a file wins. Apply: for each model, entries of that file
with `total > 0` and same base model get `explicitCost = cost * total_i / Σtotal`, last entry
absorbs the remainder (`max(0, share)`), `costIsEstimate = false` (→ `.source`). Models with no
entries are skipped. Ledger is stored separately in the blob and re-applied on read so a later
ledger can replace it.

**Incremental read (#338)** (`LocalUsageCache.swift:408-429`): blob `(mtime,size,entries,offset,claudeCostState)`.
- same mtime & size → reuse.
- file grew (`size > prior.size`) and `prior.offset` exists → parse from `offset`, merge
  `dedupKeepMax(prior.entries + tail)`, `costState = tail.costState ?? prior.costState`.
- otherwise (shrink, same-size rewrite, no offset) → parse from 0.
- `consumed` stops after the last `\n`; a trailing unterminated line is parsed now but re-read
  next time (dedup makes the repeat harmless).
- read failure: keep prior blob under its old signature (retry next refresh).

**Account attribution** (`ClaudeAccountUsage.swift`) — see §4.1a.

#### 4.1a Claude accounts & attribution (inputs to usage split)

- `additionalClaudeConfigDirs` (Settings, comma/newline list) — `ClaudeAccountRoots.swift:11-40`:
  trim, `~`/`~/` → home, must be absolute, existing directory, not a default root
  (`~/.claude`, `~/.config/claude`), dedup.
- Discovery (`:45-56`): `CLAUDE_CONFIG_DIR` entries, then sorted `~/.claude-*` / `~/.claude_*`
  having a login (`.claude.json` → `oauthAccount` object; `emailAddress`, `organizationName`,
  `organizationUuid`). Parsed lazily, cached on `(mtime,size)`.
- `pathKey(root) = first 8 hex of SHA-256(standardized path)` (`:180-187`).
- `history.jsonl` per login: lines with `"sessionId"`; `timestamp` ms number (>0) or ISO string;
  incremental byte-offset reader that only resumes when byte `offset-1` is `\n` (`:161-206`).
- Owner of a turn (`:49-65`): across accounts (in order) and the turn's sorted session ids, the
  latest prompt time `<= turn.date` wins (strictly greater replaces); if none, the account with
  the earliest first prompt; else unattributed (`""`).
- `usage(...)` (`:70-120`) returns per-account today/month tokens+cost and per-account 5 h
  blocks (official 5 h window `running(reset)` ⇒ turns `>= reset-5h`, `endTime = reset`;
  `notStarted`/expired ⇒ no block; unknown ⇒ rolling local block).
- Production gating: discovery/history/default identity are read only in the bundled app
  (`AppEnv.isBundledApp`), never in tests.

### 4.2 Codex (`codex`)

**Roots**: `normalizedRoots([~/.codex/sessions, ~/.codex/archived_sessions])` ∪ custom
(`LocalUsageReader.swift:294-325, 134-141`). No env override in v2.5.7 (Codex itself honors
`CODEX_HOME` — see Open questions).

| | macOS | Windows |
|---|---|---|
| active | `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` | `%USERPROFILE%\.codex\sessions\…` (c) |
| archived | `~/.codex/archived_sessions/rollout-*.jsonl` | `%USERPROFILE%\.codex\archived_sessions\…` (c) |

**Files**: every `*.jsonl` (not hidden) under the roots, **all** of them listed with mtime/size
(`codexRolloutFiles`, `:914-937`), sorted by path. Window files = `mtime >= since`; the rest are
parent candidates.

**Streaming line reader** (`:857-891`): 1 MiB chunks, only complete lines, empty lines skipped,
final unterminated buffer emitted.

**Per-line markers** (byte search, `:893-895`): `session_meta`, `"model"`, `token_count`.

**session_meta** (`:1033-1046`): `type == "session_meta"`, `payload`:
- `id = payload.id ?? payload.session_id` (non-empty);
- `parentID = payload.forked_from_id ?? payload.parent_thread_id` (non-empty);
- `date = timestamp`;
- `isSubagent = payload.thread_source == "subagent" || payload.source.subagent != nil`.
The **first** meta defines the rollout's `sessionID/parentSessionID/forkedAt/isSubagent`; every
meta with a new `id` switches `currentSessionID` and resets the identical-state guard (forks embed
the parent's meta after their own).

**model** (`:2005-2011`): `payload.model ?? payload.turn_context.model`; default `"codex"` until seen.

**token_count** (`:1345-1390`): `payload.type == "token_count"`, `payload.info.last_token_usage`, `timestamp`.
```
inputTotal = last.input_tokens; cached = last.cached_input_tokens; output = last.output_tokens
nonCached  = max(0, inputTotal - cached); lastTotal = last.total_tokens
if nonCached+cached+output == 0 && lastTotal > 0 && shouldTrust(lastTotal, cumulative):
    input = lastTotal; output = 0; cacheRead = 0
else: input = nonCached; output = output; cacheRead = cached
cacheWrite = 0   // reasoning_output_tokens is already inside output_tokens
costUnavailable = (nonCached+cached+output == 0 && lastTotal > 0)
id (provisional) = "codex|<file name>|<turn index>"
usageState = info.total_token_usage ? (cumulative: vec(total_token_usage), last: vec(last)) : nil
shouldTrust: no cumulative → true; cumulative components 0 and cumulative.total > 0 → true;
             else cumulative.total == lastTotal
```
Vector = `(input_tokens, cached_input_tokens, cache_write_input_tokens, output_tokens,
reasoning_output_tokens, total_tokens)`; `fingerprint` = comma join; state fingerprint =
`cum|last`; `billableComponents = max(0,input-cached)+cached+output`;
`isLower(than:)` = any component strictly lower.

**Identical-state guard** (`:822-834`): within the same `currentSessionID`, a token_count whose
full state equals the previous one is dropped. The turn index still advances for a dropped line
(`defer { turn += 1 }` at `:820` is registered before the guard). Events without state reset the guard.

**Parent closure** (`expandCodexParentClosure`, `:946-1007`):
1. Load every window file. `pending = parentSessionIDs`.
2. For each unsearched parent id not already among loaded rollouts' `sessionID`:
   a. hinted candidates among unloaded files = known session id `== parentID`, or (unknown id)
      filename contains parentID and `isUsableFilenameHint(parentID)`; load and adopt those whose
      parsed `sessionID == parentID` (add their parent id to pending);
   b. if none adopted: probe every unloaded, unhinted file whose id is unknown
      (`probeCodexRolloutSessionID`, first 1 MiB, complete UTF-8 lines only; first
      `session_meta` → id; a `token_count` before meta → nil; invalid UTF-8 line → nil;
      I/O error → throw = not cached) and adopt matches.
3. Return all loaded rollouts + `includedPaths` = window file paths only.

**Resolution** (`resolveCodexRollouts`, `:1134-1200`), memoized per path, cycle-safe:
- For a rollout with `parentSessionID`: for every candidate rollout with that session id (sorted
  by path, not itself), resolve it recursively, then
  `replayCount = comparableUsagePrefixCount(child.events, parent.history)` — count of leading
  positions where both have `usageState` and they are equal; nil if any compared side lacks state
  or the parent history is empty; 0 if child empty. Only `replayCount > 0` candidates count;
  best = largest (first wins ties).
- best found → drop `replayCount` events, inherit `parent.history.prefix(replayCount)` as history.
- parent id but no match → `fallbackReplayCount`: subagent ⇒ 0; else the first event plus every
  following event whose gap to the previous is `< 1 s` (1 event ⇒ 1, 0 ⇒ 0).
- no parent → 0.
- `resolveOwnedEvents` (`:1251-1300`): owner = `rollout.sessionID` for forks, else
  `event.sessionID ?? rollout.sessionID`. Owner change resets `epoch=0` and previous cumulative.
  A cumulative lower than the previous one ⇒ `epoch += 1`. For events with owner and state:
  apply #278 mid-session fill (`:1304-1327`: entry total 0, `last.billable == 0`, `last.total > 0`,
  previous cumulative exists and `cumulative.total > previous.total` ⇒ `input = last.total`,
  `costUnavailable = true`), then canonical
  `id = "codex|<owner>|<epoch>|<cum fingerprint>|<last fingerprint>"`. Legacy events (no state or
  no owner) keep the positional id.
- Output = owned entries of `includedPaths` (sorted) → `dedupCodexCanonicalEntries` (keep earliest).

**Cache** (`LocalUsageCache.swift:448-534`): parsed rollouts (not entries) keyed by
`(path, mtime, size)`; a separate `codexSessionIDs` index `{path: (mtime,size,sessionID?)}` that
survives the 40-day prune and is cleaned only of paths that no longer exist (and only when the
enumeration returned ≥1 file). A probe failure is remembered only for the current call.

### 4.3 Gemini CLI (`gemini`)

Roots: `~/.gemini/tmp` ∪ custom (`:143-150`). Files `*.jsonl` and `*.json` (`allowJSON`) —
sessions are `~/.gemini/tmp/<hash>/chats/session-*.jsonl` (new) or legacy `.json`.
Windows: `%USERPROFILE%\.gemini\tmp` (c). No env override.

Parse (`:1414-1460`):
- JSONL: per line containing `"tokens"` or `"timestamp"`; remember the last seen timestamp as
  fallback; absorb any object with `tokens` (records of `type:"gemini"` and `type:"message_update"`
  — the last record for an `id` wins).
- Legacy JSON: `{ startTime, messages:[…] }`, fallback date = `startTime`.
- `id = "gemini|<file name>|<obj.id ?? random UUID>"`, `model = obj.model ?? "gemini"`.
- `input = max(0, tokens.input - tokens.cached) + tokens.tool`, `cacheRead = tokens.cached`,
  `output = tokens.output + tokens.thoughts`, `cacheWrite = 0` (Σ = `totalTokenCount`).
- Global `dedupKeepMax`. Cost: price-table estimate.

### 4.4 Antigravity (`antigravity`)

Roots (`LocalAntigravityUsageReader.swift:54-78`): `~/.gemini/antigravity/conversations`,
`~/.gemini/antigravity-cli/conversations`, `~/.gemini/antigravity-ide/conversations` ∪ custom.
Windows: `%USERPROFILE%\.gemini\antigravity*\conversations` (u). Files: direct children ending
`.db`, sorted (no recursion).

- **Signature** (`:178-190`) = newest mtime of `{db, db-wal}` and summed size; **never** `-shm`.
  Stat before reading. Skip stores whose signature mtime `< since`.
- **Open** (`:358-376`): URI `file:<percent-encoded path>?mode=ro`, then `?immutable=1`;
  each must pass `SELECT count(*) FROM sqlite_master` to count as opened.
- `SELECT idx, data FROM gen_metadata WHERE data IS NOT NULL ORDER BY idx`; prepare
  `SQLITE_ERROR` ⇒ `.notAConversation` (cached empty); other errors ⇒ `.unreadable`.
- Step dates: `SELECT metadata FROM steps WHERE metadata IS NOT NULL ORDER BY idx`; date =
  Timestamp field 8 (`finished_at`) else field 1 (`created_at`); index by response id
  (field 9 → message, field 11 string) and by execution id (field 12, list in order).
- Protobuf paths in `gen_metadata.data`: `1` chat_model; `1.4` usage; `1.4.2` input (excludes
  cache reads); `1.4.3` output (thinking + response); `1.4.4` cacheWrite (never written);
  `1.4.5` cacheRead; `1.4.11` response_id; `1.9.4` created_at Timestamp; `1.19` response_model;
  `4` execution_id.
- Date: `created_at` if present & plausible; if absent → step date by response id → next unused
  date of the execution id list (clamped to last) → store mtime; if present but implausible → drop.
- `id = "antigravity|<response_id>"` else `"antigravity|<conversation file stem>|<idx>"`;
  `model = "antigravity/" + (response_model ?? "unknown")` (unpriced → cost Unavailable).
- Counter `> 1e9` ⇒ discarded (counted for a log line), absent ⇒ 0. All-zero ⇒ no entry.
- Scan outcome: `complete` → blob; `notAConversation` → empty blob; `incompleteScan`/`unreadable`
  → keep previous blob under its **old** signature (retry next time). BUSY mid-step ⇒ incomplete.
- Cache (`:656-707`): in-memory actor; blobs replaced by the visited set each scan (prune);
  concurrent callers join the in-flight task; result filtered by `enrichmentScanStart(now)`
  re-sampled after the scan; `dedupKeepMax` across stores.

### 4.5 OpenCode (`opencode`)

Roots: `OPENCODE_DATA_DIR` (comma list) else `~/.local/share/opencode`, ∪ custom
(`LocalAdditionalUsageProvider.swift:353-360`). Windows: `%USERPROFILE%\.local\share\opencode` (u —
OpenCode uses `xdg-basedir`, which falls back to `~/.local/share` on every OS).

- DB choice (`:483-496`): root itself if `*.db`; else `opencode.db`; else first (by path)
  `opencode-<channel>.db` where channel is non-empty ASCII `[A-Za-z0-9_-]`.
- V2: `SELECT id, time_created, data FROM session_message WHERE type='assistant' AND time_created >= ?1`
  (`?1 = since` ms). JSON: `tokens{input,output,reasoning,cache{read,write}}`, `model{id,providerID}`,
  `cost`. `id = "opencode|<row id>"`, `output = output + reasoning`, date = `time_created`.
- V1: `SELECT id, session_id, data FROM message WHERE time_created >= ?1`; if that fails
  (old schema) `SELECT id, session_id, data FROM message`. JSON: `tokens{input,output,total,
  cache{read,write}}`, `time.created`, `modelID`, `providerID` (required), `cost`, `id`.
  `id = "opencode|<data.id ?? row id>"`.
- Legacy files: `<root>/storage/message/**/*.json` (mtime ≥ since), same V1 JSON,
  fallback id = file stem.
- `makeEntry` (`:1694-1722`): clamp negatives; if `total > parts` the difference goes to output and
  `costUnavailable = true`; all-zero ⇒ nil. `explicitCost = cost` (`.source`).
- Filter `date >= since`, `dedupKeepMax`.
- Incremental: `since = max(periodStart, newestCachedDate - 1 s)`, merge `dedupKeepMax(existing + loaded)`.

### 4.6 Hermes Agent (`hermes`)

Roots: `HERMES_HOME` (comma list) else `~/.hermes`, ∪ custom (`:367-374`). DB = root if `*.db`
else `<root>/state.db`. Windows: `%USERPROFILE%\.hermes` (u).

```sql
SELECT id, model, billing_provider, started_at, message_count,
       input_tokens, output_tokens, cache_read_tokens, cache_write_tokens,
       reasoning_tokens, estimated_cost_usd, actual_cost_usd
FROM sessions WHERE model IS NOT NULL AND TRIM(model) != '' AND started_at >= ?1   -- seconds
```
`id = "hermes|<id>"`, date = `started_at`, `input=input_tokens`,
`output = output_tokens + reasoning_tokens`, `cacheWrite=cache_write_tokens`,
`cacheRead=cache_read_tokens`, cost = `actual ?? estimated`, `costIsEstimate = actual == nil`,
`costUnavailable = true` (session totals). Dedup by id across roots (first wins). Full re-query each 30 s.

### 4.7 Cursor (`cursor`)

Roots: `CURSOR_DATA_DIR` (comma list) else
`~/Library/Application Support/Cursor/User/globalStorage` and
`~/Library/Application Support/Cursor Nightly/User/globalStorage`, ∪ custom (`:784-800`).
DB = root if `*.vscdb` else `<root>/state.vscdb`.
Windows: `%APPDATA%\Cursor\User\globalStorage\state.vscdb` (c),
`%APPDATA%\Cursor Nightly\User\globalStorage` (u).

**A. Dashboard API (authoritative when it answers)** — `CursorUsageAPI.swift`:
- Disabled when `CURSOR_USAGE_API == "0"`.
- Token: `CURSOR_SESSION_TOKEN`, else `ItemTable` key `cursorAuth/accessToken` in the first root
  that has it (never mix Stable access with Nightly refresh).
- `POST https://cursor.com/api/dashboard/get-filtered-usage-events`, JSON body
  `{"teamId":0,"startDate":"<ms since>","endDate":"<ms now>","page":n,"pageSize":100}`
  (strings of epoch ms — ISO makes the server 500). Headers: `Content-Type: application/json`,
  `User-Agent: Mozilla/5.0`, `Origin: https://cursor.com`, `Referer: https://cursor.com/dashboard/usage`.
  Auth mode 1: `Cookie: WorkosCursorSessionToken=<sub>::<jwt>` (sub from JWT payload when token
  has no `::`); on 401/403 switch to `Authorization: Bearer <part after ::>`; both rejected →
  one token refresh then retry.
- Events: `usageEventsDisplay ?? usageEvents ?? events`; next page by
  `pagination.hasNextPage` → `pagination.numPages` → `totalUsageEventsCount` → full page.
- Event → entry: date from `timestamp` (numeric string/number: `>1e12` ms, `>=1e9` s; else ISO);
  `model ?? "unknown"`; `tokenUsage{inputTokens,outputTokens,cacheWriteTokens,cacheReadTokens,totalCents}`;
  cost = `totalCents/100` (`.source`); `id = "cursor|api|" + (id ?? eventId ?? requestId)` else
  `"cursor|api|<timestamp>|<model>|<rowIndex>"`.
- Disk cache `cursor-usage-api-cache.json` in state dir: `{fetchedAt, accountIdentifier,
  coveredSince, entries}`; account id = `subject:<sub>` or `token:<sha256>`. On failure use cache
  if same account, ≤ 6 h old; authoritative only if `coveredSince <= since`.
- An authoritative result (even empty) **replaces** everything, no merge with local bubbles.

**B. Local bubbles (fallback)**: incremental store scan with `MAX(rowid) FROM cursorDiskKV`.
Cold: `SELECT rowid, key, value FROM cursorDiskKV WHERE key GLOB 'bubbleId:*'`; incremental:
`... FROM cursorDiskKV NOT INDEXED WHERE rowid > ?1 AND key GLOB 'bubbleId:*'`. Bubble JSON:
`tokenCount{inputTokens,outputTokens}` (sum > 0), `createdAt` (ISO or epoch), `modelType ?? "unknown"`;
`id = "cursor|<key>"`, `costUnavailable = true`. Cache merge drops prior `cursor|api|` entries.

### 4.8 Grok CLI (`grok`)

Roots: `$GROK_HOME/sessions` else `~/.grok/sessions`, ∪ custom (`:152-165`).
Windows: `%USERPROFILE%\.grok\sessions` (u). Files: only `updates.jsonl` whose session dir's
`summary.json.session_kind` does **not** start with `subagent` (evaluated before cache).

Line (`:1573-1664`) must contain `turn_completed`; envelope
`{timestamp, method:"_x.ai/session/update", params:{sessionId, update:{…}, _meta:{…}}}` (old lines
have no envelope). Require `update.sessionUpdate == "turn_completed"`, `update.usage`,
`update.prompt_id` (non-empty) ; skip if `_meta.isReplay`.
- `output = outputTokens ?? output_tokens ?? 0`; `cr = cachedReadTokens ?? cached_read_tokens ?? 0`.
- If `inputTokens` (camel, includes cache) present: `cacheRead = min(cr, inputTokens)`,
  `input = inputTokens - cacheRead`; else `input = input_tokens ?? 0` (already excludes cache), `cacheRead = cr`.
- If `totalTokens ?? total_tokens` > parts → excess added to output. All-zero ⇒ no entry.
- Date: `_meta.agentTimestampMs` (ms) > 0, else envelope `timestamp` (≥1e11 ⇒ ms) else ISO string.
- `id = "grok|<prompt_id>"`; model = key of `usage.modelUsage|model_usage` with the largest
  `totalTokens|total_tokens` (keys sorted, first max) else `"grok"`.
- Cost = `costUsdTicks|cost_usd_ticks / 1e10` unless `usageIsIncomplete|usage_is_incomplete` or
  `costIsPartial|cost_is_partial` (then nil → table, which has no Grok rows → unavailable). `.source`.
- Global `dedupKeepMax` (fork copies keep `prompt_id`).

### 4.9 GitHub Copilot CLI (`copilot`)

Roots: `COPILOT_HOME` (comma list) else `~/.copilot`, ∪ custom; DB = root if `*.db` else
`<root>/session-store.db` (`:1054-1111`). Windows: `%USERPROFILE%\.copilot\session-store.db` (c).
- Watermark `SELECT MAX(id) FROM assistant_usage_events`.
- Rows: `SELECT id, model, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, created_at
  FROM assistant_usage_events WHERE id > ?1 AND created_at >= ?2`, `?2 = "<UTC day before since> 00:00:00"`.
- `created_at`: `"YYYY-MM-DD HH:MM:SS"` (UTC) or ISO; space→`T`, append `Z` if no zone; < 19 chars ⇒ drop.
- `input = max(0, input_tokens - cache_read - cache_write)`; reasoning not added.
- `id = "copilot|<db path>|<id>"`; cost: price table.

### 4.10 Kiro CLI (`kiro`)

Roots (`:1240-1250`): SQLite home `KIRO_CLI_HOME` else `~/Library/Application Support/kiro-cli`
(DB `data.sqlite3`), plus `($KIRO_HOME ?? ~/.kiro)/sessions`, ∪ custom.
Windows: `%LOCALAPPDATA%\kiro-cli\data.sqlite3` (u — Rust `dirs::data_local_dir`), `%USERPROFILE%\.kiro\sessions` (u).
Tokens are a **bytes/4 estimate**; `costUnavailable = true` always; credits ignored.

- Skip unchanged files by signature (`db` + `-wal`) remembered in the cache (`kiroSignatures`);
  unchanged ⇒ `[]` and the sticky merge keeps prior entries.
- SQLite: `SELECT conversation_id, value FROM conversations_v2` and `SELECT value FROM conversations`;
  both failing ⇒ retry next poll (no signature). Per conversation JSON: `history[]` of
  `{user, assistant, request_metadata{request_start_timestamp_ms, model_id, response_size}}`,
  seed `cumulative = bytes(latest_summary)`; `input = (cumulative + userBytes)/4`,
  `output = response_size/4`; history accumulates even for skipped turns;
  `id = "kiro|<conversation_id>|<ms>"`. Field bytes skip the `images` key.
- CLI JSONL `sessions/cli/<id>.jsonl` + companion `<id>.json` (`session_id`,
  `session_state.rts_model_state.model_info.model_id`): kinds `Prompt|AssistantMessage|ToolResults|Clear`;
  `input = (history + prompt + tool)/4`, `output = assistant/4`, `id = "kiro|cli|<session>|<ms>"`.
- v3 `messages.jsonl` + sibling `session.json` (`id`, `modelId`, `createdAt|lastModifiedAt`):
  payload types `user|assistant|tool_call(args→output)|tool_result(→input)|turn_end`, or flat
  `role` lines; `id = "kiro|v3|<session>|<turnIndex>"`.
- Cache merge `dedupKeepMax(existing + loaded)` (deleted turns stay counted until restart/month change).

### 4.11 Pi (`pi`)

Roots: `~/.pi/agent/sessions`, `$PI_CODING_AGENT_DIR/sessions`, `$PI_CODING_AGENT_SESSION_DIR`,
normalized, ∪ custom (`:335-349`). Windows: `%USERPROFILE%\.pi\agent\sessions` (u).
Lines containing `"usage"` with envelope `id` and `type`:
- `message`: skip `stopReason` `aborted|error`; `usage = message.usage`; date = `message.timestamp`
  ms else envelope ISO `timestamp`; `model = envelope.model ?? message.model ?? "pi"`.
- `compaction|branch_summary`: envelope `usage`, envelope timestamp.
- Buckets `input,output,cacheWrite,cacheRead` (any present ⇒ granular); else `totalTokens` → input
  with `costUnavailable`. `explicitCost = usage.cost.total`, `costIsEstimate = true`.
- `id = envelope.id` (global — fork copies collapse). `includeModels`.

### 4.12 omp (`omp`)

Roots `~/.omp/agent/sessions` + `$OMP_CODING_AGENT_DIR/sessions` ∪ custom; exclude any path with a
`bridge` component. Only `message` with `role == "assistant"` (plus compaction/branch_summary);
`model = message.model ?? "omp"`; `id = "omp|<file name>|<envelope.id ?? UUID>"`; cost
`usage.cost.total` finite ≥ 0 (estimate). Windows `%USERPROFILE%\.omp\agent\sessions` (u).

### 4.13 omo / senpi (`omo`)

Roots `~/.omo/agent/sessions`, `~/.senpi/agent/sessions`, `$OMO_CODING_AGENT_DIR/sessions`,
`$SENPI_CODING_AGENT_DIR/sessions` (not `PI_CODING_AGENT_DIR` — avoids double count) ∪ custom.
As omp, plus `role == "toolResult"` with `usage`; `cacheWrite = cacheWrite + cacheWrite1h`;
`id = "omo|<parent dir>/<file>|<envelope.id ?? UUID>"`. Windows `%USERPROFILE%\.omo\…`, `\.senpi\…` (u).

### 4.14 Aside (`aside`)

Roots `~/.aside/u` ∪ custom; DBs = `<root>/state.db` and `<root>/<child>/state.db`
(`LocalAsideUsageReader.swift:17-34`). Windows: `%USERPROFILE%\.aside\u` (u — Aside may be macOS-only).
```sql
SELECT t.id, t.token_usage, COALESCE(t.finished_at, t.last_message_timestamp), s.model
FROM session_turns t LEFT JOIN sessions s ON s.id = t.session_id
WHERE COALESCE(t.finished_at, t.last_message_timestamp) >= ?   -- seconds (double)
```
`token_usage` JSON buckets `input,output,cacheRead,cacheWrite` (else `totalTokens` → input);
`model = sessions.model JSON .modelId ?? "aside"`; `explicitCost = cost.total`;
`costUnavailable = true`; `id = "aside|<path>#<inode>:<turn id>"`. Unopenable/failed DBs skipped
(partial rows dropped). Sticky merge. `includeModels` off.

### 4.15 Kimi Code (`kimi_code`)

Roots `~/.kimi-code/sessions`, `~/Library/Application Support/kimi-desktop/daimon-share/daimon/runtime/kimi-code/home/sessions`,
`$KIMI_CODE_HOME/sessions` ∪ custom. Files named `wire.jsonl` (any depth). Lines with
`"usage.record"`: `type == "usage.record"`, `usage`, `time` ms > 0, `usageScope ∈ {nil,"turn","session"}`.
`input=inputOther`, `output=output`, `cacheRead=inputCacheRead`, `cacheWrite=inputCacheCreation`,
`model ?? "kimi-code"`; content id
`"kimi|<ms>|<model>|<scope ?? "-">|<in>|<out>|<cr>|<cw>"`. Cost unpriced. `includeModels`.
Windows: `%USERPROFILE%\.kimi-code\sessions` (u), desktop `%APPDATA%\kimi-desktop\…` (u).

### 4.16 Devin CLI (`devin`)

Roots `($XDG_DATA_HOME ?? ~/.local/share)/devin` ∪ custom; DB = root if `*.db`, `<root>/sessions.db`
if root is `cli`, else `<root>/cli/sessions.db`. Watermark `MAX(row_id) FROM message_nodes`.
```sql
SELECT m.row_id, m.chat_message, m.session_id, m.created_at, s.model
FROM message_nodes m LEFT JOIN sessions s ON s.id = m.session_id
WHERE m.row_id > ?1 AND m.created_at >= <since_seconds - 86400>
```
`chat_message.metadata.metrics{input_tokens,output_tokens,cache_creation_tokens,cache_read_tokens}`
(input already excludes cache); `id = "devin|<db>|<session ?? nosession>|<message_id ?? row<rowid>>"`;
`costUnavailable = true`. Windows: `%LOCALAPPDATA%\devin\cli\sessions.db` or
`%USERPROFILE%\.local\share\devin\…` (u).

### 4.17 Incremental SQLite rules (Cursor / Copilot / Devin) — `LocalAdditionalUsageProvider.swift:648-770`

- `MAX(...)` failure ⇒ keep previous watermark, no rows, not a reset.
- `didReset = after > 0 && max < after`; then cold scan **every** root and replace the cache.
- Query failure (BUSY/interrupt — any `sqlite3_step` other than ROW/DONE) ⇒ no rows, keep watermark.
- New watermark = max row id seen; `0` ⇒ keep `effectiveAfter`.
- Output filtered `date >= since`, `dedupKeepMax`; cache merges `dedupKeepMax(existing + loaded)` unless reset.
- All SQLite opens are `SQLITE_OPEN_READONLY | SQLITE_OPEN_NOMUTEX`; missing file ⇒ `[]` (not failure).

---

## 5. Data / persistence

| Key / file | Type | Default | Notes |
|---|---|---|---|
| `disabledProviderIDs` | `[String]` sorted | `[]` | opt-out; new providers on by default |
| `customScanRoots.<providerID>` | `String` | absent | raw text, comma/newline; whitespace-only = nil |
| `additionalClaudeConfigDirs` | `String` | absent | Claude extra logins |
| `<binary>Path` | `String` | absent | BinaryLocator manual override |
| `~/Library/Application Support/PokeTokenBar/usage-cache.json` | zlib JSON `Snapshot` | — | written only by the bundled app |
| `<state dir>/cursor-usage-api-cache.json` | JSON | — | Cursor API cache |

Snapshot (`LocalUsageCache.swift:55-124`): maps `claude, codex, codexSessionIDs, gemini, grok, pi,
omp, omo, kimi` keyed by absolute path + versions. Current versions: claude 1, codex 6,
codexSessionIndex 2, grok 2, pi 3, omp 2, omo 1, kimi 1 (`:129-147`); a mismatch drops only that
map. Gemini has no version. Every key decodes leniently (missing ⇒ empty).
`Entry` JSON keys: `id,date,localDay,model,input,output,cacheWrite,cacheRead,explicitCost,
costIsEstimate,costUnavailable,sessionID,sessionIDs` (Swift `Date` encodes as seconds since 2001).

---

## 6. UI spec

### 6.1 Tracked providers (Settings, main scroll) — `UI/SettingsView.swift:305-317`

```
VStack(spacing 6, leading)
├─ Section title  "TRACKED PROVIDERS"   caption(10pt) semibold, uppercase, secondary, padding-left 4
├─ Card  (VStack spacing 0; bg controlBackground; radius 8; 1px stroke separator @60%)
│   └─ for each of 16 providers in registration order:
│        Divider (not before the first)
│        Row: HStack spacing 10, padding 12h/8v, min-height 38
│            Label (body 13pt, labelColor)      Spacer      Switch (small)
└─ Hint  caption2(10pt) tertiary, padding-left 4
```
- Labels are `displayName`: Claude Code, Codex, Gemini, Antigravity, OpenCode, Hermes Agent,
  Cursor, Grok, Copilot, Kiro, Pi, omp, omo, Aside, Kimi Code, Devin.
- Hint text (en): "Turned-off providers are not read. They are excluded from tabs, totals, Pokémon
  growth, and limits." (ko: "끈 프로바이더는 로그를 읽지 않아요. 탭·합계·포켓몬 성장·한도에서 빠져요.",
  ja: "オフにしたプロバイダーのログは読みません。タブ・合計・ポケモンの成長・上限から外れます。").
  Title ko "추적할 프로바이더", ja "集計するプロバイダー".
- Toggle off: write set, remove the tab immediately, trigger refresh. Toggle on: refresh.
- Screenshot `assets/screenshot-provider-settings.png` (dark, 2×): rows ≈78 px @2× = 39 pt.

Colors (macOS system colors, approximate hex):

| Token | Light | Dark |
|---|---|---|
| card bg `controlBackgroundColor` | `#FFFFFF` | `#1E1E1E` |
| card stroke `separatorColor`×0.6 | `rgba(0,0,0,0.06)` | `rgba(255,255,255,0.06)` |
| divider | `rgba(0,0,0,0.10)` | `rgba(255,255,255,0.10)` |
| label | `rgba(0,0,0,0.85)` | `rgba(255,255,255,0.85)` |
| section title (secondary) | `rgba(0,0,0,0.50)` | `rgba(255,255,255,0.55)` |
| hint (tertiary) | `rgba(0,0,0,0.26)` | `rgba(255,255,255,0.25)` |
| switch on | `#007AFF` | `#0A84FF` (screenshot shows gray only because of the inactive render) |
| switch off track | `rgba(0,0,0,0.09)` | `rgba(255,255,255,0.10)` |
| window bg | `#ECECEC` | `#2B2B2B` |

### 6.2 Additional scan folders (Settings → Advanced · diagnostics) — `UI/SettingsView.swift:790-834, 979-1004`

```
groupRow (VStack spacing 6, leading)
├─ "Additional scan folders"                      body
├─ hint                                           caption2 tertiary
├─ HStack: "Provider" (caption)  Spacer  Picker(menu, fixed size) of 16 displayNames
├─ TextField(axis vertical, rounded border, caption font, placeholder "~/path/to/sessions")
└─ "Scans N extra folder(s) now"  caption2 tertiary — only when draft is non-blank
```
- Hint (en): "Only for this provider's logs outside the built-in locations. Comma/newline
  separated, * wildcards. Do not point at another provider's folder."
- Selected provider default `claude_code` (not persisted).
- Draft is owned by `customScanDraftOwnerID`. Commit on Enter, on focus loss, on picker change
  (save the old draft to the **old** owner first), and on disappear. Save only if changed; save ⇒
  invalidate Claude roots cache + additional-source cache, then refresh.
- Match count = `survivingExtraCount(curatedRoots(provider), draft)`, recomputed on a background
  task after 250 ms debounce, discarded if a newer generation started.
- Below it in the same card: divider + footnote "Token basis: totalTokens (input + output + cache,
  local date)" caption2 tertiary, padding 12h/8v.
- Screenshot `assets/screenshot-scan-folders.png`.

### 6.3 States

- Provider without data → no tab (no empty state per provider). All providers disabled → no
  retry is scheduled. Errors per provider are joined `"<id>: <error>"` with `" / "` in
  `lastErrorDescription`; a failed daily keeps the previous today value; failed enrichment keeps
  previous periods.

---

## 7. Windows adaptation

| macOS-only behaviour | Windows / Tauri equivalent |
|---|---|
| Login-shell env lookup (`zsh -ilc`, 8 s, `UsageEnvironment`) because Finder apps miss `~/.zshrc` exports | Read process env; for names still missing read `HKCU\Environment` then `HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment` (values set by `setx` after the app started are only there). Expand `REG_EXPAND_SZ` (`%USERPROFILE%`). Resolve once per process; same 19 names. Use `winreg` crate or `reg query`. |
| `BinaryLocator` (static paths + shell `command -v`) | Not needed for usage. For other areas: settings override → `where.exe <bin>` → `%APPDATA%\npm`, `%LOCALAPPDATA%\Programs`, `%USERPROFILE%\.bun\bin`, `%USERPROFILE%\scoop\shims`, `%LOCALAPPDATA%\Volta\bin`; try `.exe`, `.cmd`. |
| `UserDefaults` keys | Fields in `settings.json` (app_data_dir): `disabled_providers: Vec<String>`, `custom_scan_roots: HashMap<String,String>`, `additional_claude_config_dirs: String`. Keep id strings identical. |
| `~/Library/Application Support/PokeTokenBar/usage-cache.json` (Apple `.zlib` = raw DEFLATE) | `app.path().app_data_dir()/usage-cache.json` (`%APPDATA%\<identifier>`). Plain JSON or `flate2` DEFLATE; no cross-platform compatibility needed. Write atomically (temp + rename). |
| `FileManager.enumerator(.skipsHiddenFiles)` | `walkdir`/`read_dir` recursion skipping entries whose name starts with `.` (and optionally `FILE_ATTRIBUTE_HIDDEN`). Never skip the root itself (`.claude` is a root). |
| `.skipsPackageDescendants` | No-op on Windows. |
| `resolvingSymlinksInPath` / APFS case-insensitivity | `dunce::canonicalize` (resolves symlinks and junctions, strips `\\?\`); compare lowercased; nest test with `\` **and** `/` separators. |
| Tilde expansion, absolute check `hasPrefix("/")` | `~`/`~\`/`~/` → `%USERPROFILE%`; absolute = `Path::is_absolute()` (drive or UNC). Accept `/` and `\`. |
| `fnmatch` per segment | `glob` crate or a small `*`,`?`,`[...]` matcher, case-insensitive; enumerate sorted directory names. |
| `stat().st_ino` (Aside store identity) | `std::os::windows::fs::MetadataExt::creation_time()` (stable) or file index via `GetFileInformationByHandle`. |
| System `libsqlite3` | `rusqlite` with `bundled`; open `OpenFlags::SQLITE_OPEN_READ_ONLY | SQLITE_OPEN_NO_MUTEX | SQLITE_OPEN_URI`; URI form `file:///C:/path?mode=ro`. Windows file locking can make `mode=ro` fail while Cursor writes — keep the retry rules. |
| `URLSession` (Cursor API) | `ureq` (already a dependency) on a worker thread; 10 s timeout. |
| `ProcessInfo.beginActivity` (App Nap) | Not needed. |
| `AppEnv.isBundledApp` gates (no real logs in tests) | `#[cfg(not(test))]` or an explicit `UsageRoots` injected in tests. |
| `NSLock` / Swift `actor` caches | `Mutex<Cache>` in Tauri state; run scans on `tauri::async_runtime::spawn_blocking`. |
| Keychain (`ClaudeAccountRoots.keychainServices`) | Claude Code on Windows stores `.credentials.json` in the config dir; no Keychain service names needed. (Limits area.) |
| NSStatusItem / NSPanel / notifications / LaunchAgent / URL schemes | Not used by providers. |

Paths to use on Windows: §4 tables. Env lists stay comma-separated (as macOS).

---

## 8. Gaps vs current Windows code (`src-tauri/src/usage.rs`)

Current state: today-only Claude + simplified Codex, full rescan, totals only.

1. **14 providers missing**: Gemini, Antigravity, OpenCode, Hermes, Cursor, Grok, Copilot, Kiro,
   Pi, omp, omo, Aside, Kimi Code, Devin. No `rusqlite` dependency yet (needed by 8 of them).
2. **No Entry model / aggregation**: no `date`, `localDay`, `model`, cost fields; no week, month,
   month series, 21-day series, 5 h block, `lastUsage`, per-model breakdown, `hasDisplayableUsage`.
   `TodayUsage` hard-codes `claude`/`codex` fields; `lib.rs:213-216` credits only those two.
3. **No cost** at all (ModelPricing, Claude `cost-state`, source costs, coverage).
4. **Claude roots**: only `~/.claude/projects`. Missing `CLAUDE_CONFIG_DIR` (comma), `~/.config/claude`,
   extra login folders, Desktop embedded sessions, custom roots, `normalizedRoots`.
5. **Claude dedup**: filters by day *before* dedup, so a turn re-logged on a later day by a fork
   is counted on the later day (Swift keeps earliest date). Keeps max only within today.
   No session-id capture (needed for account attribution), no `<synthetic>` handling for block.
6. **No incremental reading (#338)**: `read_today()` re-reads every today-modified file each
   refresh (`usage.rs:137`); no persisted cache, no byte offset, no parser versions.
7. **Codex**:
   - `~/.codex/archived_sessions` not scanned (today's usage drops when a session is archived).
   - No fork/subagent replay resolution (the `ponytail` note at `usage.rs:161-162`): forked
     rollouts that replay parent `token_count` events are double counted. Requires the full §4.2
     algorithm: `session_meta` parsing (`forked_from_id`/`parent_thread_id`, subagent flag),
     parent closure over **all** files (not only today's), 1 MiB metadata probe, usage-state prefix
     match, 1 s fallback, owner/epoch canonical ids, keep-earliest dedup, session-id index.
   - No identical consecutive state dedup.
   - No #278 total-only handling (`last_token_usage` components 0, `total_tokens > 0`).
   - No model capture (`turn_context.model`) → no cost.
   - Day filter by line timestamp only; no window for enrichment.
8. **Number parsing**: `as_u64` drops floats/strings; no `1e18` clamp; no `null` distinction.
9. **No per-provider enable switch (#460)**: no `disabled_providers` setting, no Settings section.
10. **No custom scan roots (#177)**: no setting, no glob expansion, no match count, no UI.
11. **No env override resolution** (`UsageEnvironment`) — `HOME` via `std::env::home_dir()` only.
12. **No Claude per-account attribution** (`history.jsonl`) and no additional account roots.
13. **Hidden-file handling**: `jsonl_files` descends into dot-directories (Swift skips them).
14. **No empty-usage 20 s retry**, no per-provider error/keep-previous semantics.
15. **No Cursor dashboard API** (and its disk cache / auth fallback).

What already matches: Claude line field mapping and `(message.id|requestId)` keep-max; Codex
`last_token_usage` mapping `input - cached`, `cacheRead = cached`, `cacheWrite = 0`;
`compact()` formatter.

Suggested Rust layout (no extra abstractions): `usage/mod.rs` (Entry, Bucket, aggregation,
roots, number helpers), `usage/jsonl.rs` (Claude, Codex, Gemini, Grok, Pi, omp, omo, Kimi),
`usage/sqlite.rs` (OpenCode, Hermes, Cursor, Copilot, Devin, Aside, Kiro),
`usage/antigravity.rs`, `usage/cache.rs`, `usage/cursor_api.rs`. One `fn entries(id, since)`
match over provider ids instead of a trait object per provider.

---

## 9. Open questions

1. **Codex `CODEX_HOME`**: v2.5.7 ignores it (macOS too). Port as-is for parity, or add
   `$CODEX_HOME/sessions` + `archived_sessions` on Windows where custom homes are common?
2. Uncertain Windows locations to verify on real installs: Claude Desktop embedded sessions
   (`%APPDATA%\Claude\…`), Antigravity (`%USERPROFILE%\.gemini\antigravity*`), OpenCode
   (`.local\share\opencode` vs `%LOCALAPPDATA%\opencode`), Hermes, Kiro SQLite
   (`%LOCALAPPDATA%\kiro-cli` vs `%APPDATA%\kiro-cli`), Devin, Kimi desktop, Aside (may not ship
   on Windows), Cursor Nightly folder name, Pi/omp/omo/senpi.
3. Cursor dashboard API is unofficial and network-bound: include in first port or ship local
   bubbles only and add the API later?
4. Should `~/.config/claude` be scanned on Windows (no XDG convention there)? Harmless if absent.
5. Custom scan roots on Windows: also expand `%VAR%` forms? (macOS only expands `~`.)
6. Claude account attribution and per-account blocks are consumed by the limits/accounts area —
   confirm that area owns the `history.jsonl` reader so it is not ported twice.
7. Cache format: is a fresh, Windows-only `usage-cache.json` (plain JSON) acceptable, given no
   macOS cache migration is possible anyway?
