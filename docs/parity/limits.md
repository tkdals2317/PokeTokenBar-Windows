# Parity spec: Official limits

Source of truth: PokeTokenBar v2.5.7 (macOS, Swift). Paths below are relative to `Sources/PokeTokenBar/`.
Target: `src-tauri/src/limits.rs`, `src-tauri/src/lib.rs` (`refresh_limits`, `scheduler`, `update_tray`), `src/App.tsx` (`LimitsCard`), `src/Settings.tsx`.

---

## 1. Overview

"Official limits" are the vendor-reported quota percentages: Claude 5-hour / weekly / per-model weekly, Codex buckets, Antigravity buckets, Cursor monthly allowance, OpenCode Go 5h/weekly/monthly, Grok credits. They are separate from local token counting. **A failure only hides or greys out the limits section. It never affects the token totals** (`Core/OAuthLimitsProvider.swift:36`).

Every limit source is read on the same refresh cycle as local usage, after the local phases. Each source keeps its last good value when a fetch fails. Each source has its own `updatedAt`, a 15-minute stale rule, and an "auth expired" flag. All sources feed:

- popover rows (gauge, %, reset countdown, pace marker, pace-tier colour),
- menu-bar items (`Claude 62% · Codex 30%`, coloured per tier),
- warn/crit notifications (edge-triggered) and floating-pet bubbles,
- companion mood (`isLimitWarning`, `highestLimitUtilization`) and candy rewards (`candyEligibleWindows`). The companion and candy areas are specified elsewhere. This spec only defines the inputs they read.

The Windows port today has only Claude (OAuth file, legacy fields only) and Codex (parsed from rollout logs). It shows one card with a bar per window, one tray % (the peak across everything), and simple tier notifications.

---

## 2. Features

1. **Claude limits via OAuth token** (`api.anthropic.com/api/oauth/usage`). Token sources: `~/.claude/.credentials.json` first, then an in-memory cache, then the macOS Keychain (only on a manual refresh).
2. **Claude account identity / plan**: `GET /api/oauth/profile` gives email, organization name and `plan_display_name`. Results are cached per token (max 8 entries).
3. **Plan display** (`Plan Max`, `Plan Ultra`, `Plan Pro`) comes from the profile name, or is assembled from `subscriptionType` + `rateLimitTier`.
4. **Account label** (`Account a@b.com · Org`).
5. **New `limits[]` schema**: extra rows for `weekly_scoped` per-model weekly limits.
6. **claude.ai session-key path** (cookie `sessionKey`). It works without the Keychain, so automatic polls always succeed. It handles org discovery and auto-pick, lets the user pick an org manually, and validates on save.
7. **Chained provider**: the session key is tried first, then OAuth. When a session key exists, the OAuth fallback may never prompt.
8. **Additional Claude accounts** (`CLAUDE_CONFIG_DIR` folders): one tab per account, a per-folder session key, per-folder 429 backoff, and de-duplication by email+org. This is in scope only as far as the limits pipeline goes. The account-discovery details belong to the "accounts" spec.
9. **Codex limits** via `codex app-server --stdio` JSON-RPC `account/rateLimits/read`. It reports multiple buckets, a personal spend limit, credits, plan type and "limit reached".
10. **Antigravity limits** (Google Cloud Code `retrieveUserQuotaSummary`). Tokens come from token files or the Keychain (`gemini`/`antigravity`), with Google refresh-token refresh.
11. **Cursor limits** (Connect RPC `GetCurrentPeriodUsage`). The token comes from Cursor's `state.vscdb`, with OAuth refresh on 401/403.
12. **Cursor dashboard usage events** (`get-filtered-usage-events`). This is a usage source, not a limit, but it shares the Cursor auth. It is listed here for the auth contract only.
13. **OpenCode Go limits** (`opencode.ai/zen/go/v1/usage`). Uses the API key from OpenCode's DB or `auth.json`.
14. **Grok credit limits** (`cli-chat-proxy.grok.com/v1/billing?format=credits`). Uses the token from `$GROK_HOME/auth.json`.
15. **Provider status banner** (statuspage.io components "Claude Code" and "Codex API"). Display only.
16. **Polling cadence**: interval presets, coalesced refresh, suspend while the display sleeps, a 5-minute probe, refresh on wake, refresh on network reconnect (5 s cooldown), and refresh at midnight.
17. **429 backoff** (Claude): Retry-After first, else exponential 300 s → 3600 s. Manual refresh bypasses it.
18. **Stale badge**: values older than 15 minutes show "Stale · 17 min".
19. **Auth-expired banners**, one per provider with its own remedy. Values under the banner are drawn at 50 % opacity.
20. **Pace marker** (even-burn line) and the **six-tier pace colour** on gauges and menu-bar items.
21. **Used/remaining display mode** (numbers and fill only; warnings always use the real utilization).
22. **Reset label**: relative countdown plus a wall-clock suffix (`(16:57)` or `(Thursday 1, 14:57)`).
23. **5-hour depletion forecast** row.
24. **Warn/crit notifications**: edge-triggered, keyed per window, re-armed when utilization drops below warn. A floating-pet bubble shows the single worst alert for 6 s.
25. **Keychain kill switch** (`disableKeychainAccess`). This is macOS only. On Windows there is nothing to disable (see §6).
26. **Per-provider enable switch**: a disabled provider's limits are cleared and never fetched.

---

## 3. Rules and constants

### 3.1 Error enum (`Core/OAuthLimitsProvider.swift:4-28`)

`LimitsError`: `keychainAccessDisabled`, `liveFetchNotPermitted`, `keychainUnavailable(OSStatus)`, `keychainInteractionNotAllowed`, `credentialFormat`, `credentialMissingAccountOAuth`, `httpStatus(Int)`, `rateLimited(retryAfter: TimeInterval?)`, `sessionKeyMissing`, `sessionKeyMalformed`, `sessionKeyInvalid`, `sessionKeyNoOrganization`.

Retry-After parsing (`OAuthLimitsProvider.swift:103-108`): only the integer/decimal-seconds form is accepted. The value is whitespace-trimmed and must be `> 0`. It is capped at **3600**. An HTTP-date value gives `nil`.

`friendlyLimitError` (`Core/UsageStore.swift:1998-2021`) maps errors to UI text:

| error | text key |
|---|---|
| rateLimited | `limitRefreshRateLimited` ("Claude limit checks are temporarily rate-limited (429). Backing off and retrying automatically.") |
| httpStatus(401/403) | `limitRefreshHTTPError` with the "credential expired/unauthorized … If you only use Codex you can ignore this" variant |
| httpStatus(other) | "Failed to fetch Claude limits (N)." |
| keychainUnavailable, credentialFormat, sessionKeyMissing | `limitRefreshNoCredential` |
| credentialMissingAccountOAuth | `limitRefreshReauthNeeded` ("…Run `/login` in Claude Code…") |
| keychainInteractionNotAllowed, keychainAccessDisabled, liveFetchNotPermitted, non-LimitsError | `limitRefreshGeneric` |
| sessionKeyMalformed / Invalid / NoOrganization | `sessionKeyMalformedError` / `sessionKeyExpiredError` / `sessionKeyNoOrgError` |

### 3.2 Claude OAuth usage (`Core/OAuthLimitsProvider.swift`)

- `GET https://api.anthropic.com/api/oauth/usage`, timeout 15 s.
  - Headers: `Authorization: Bearer <accessToken>`, `anthropic-beta: oauth-2025-04-20` (`:85-99`).
  - Non-200: 429 throws `rateLimited(Retry-After)`. Any other status throws `httpStatus(code)`.
- On `httpStatus` 401 or 403 (`:58-68`):
  1. Invalidate the in-memory token cache.
  2. Re-acquire the token with `bypassCache: true`.
  3. If the new token equals the old one, rethrow. Otherwise retry once with the new token.
- After success: read `subscriptionType` and `rateLimitTier` from the credential used. Then call the profile endpoint (best effort) to fill `accountEmail`, `accountOrganizationName` and `planDisplayName`.
- A user-initiated refresh passes `allowKeychainPrompt=true`, which also sets `bypassCache=true` (#300: the cached old account must not mask a new login).

**Profile**: `GET https://api.anthropic.com/api/oauth/profile`, same headers, timeout 15 s. Only 200 counts.
- Parse `account.email` (required, non-empty). Without it the whole response is dropped.
- Parse `organization.name` and `organization.plan_display_name`. Both are optional and whitespace-trimmed; an empty value becomes `nil` (`:161-180`).
- Cache: `[token: identity]`, max 8 entries. When full, clear everything before inserting. Failures are not cached (`:123-141`).

**Token acquisition order** (`OAuthAccessTokenCache.accessToken`, `:211-252`):
1. Read the credentials file (`~/.claude/.credentials.json`, or `<root>/.credentials.json` for extra roots). If it holds a valid, non-expired credential, cache it and return it. **The file wins over the cache** (#227: `/login` switching accounts).
2. If `!bypassCache` and the cached credential is not expired, return the cached token.
3. If `!allowKeychainPrompt`, throw. The error is `credentialMissingAccountOAuth` when the file exists, parses as JSON and lacks a `claudeAiOauth` dictionary. Otherwise it is `keychainInteractionNotAllowed`. For the default root this check is skipped when the env var `CLAUDE_CONFIG_DIR` is non-empty.
4. Manual path (macOS only): a silent Keychain read (no-UI query), then a prompting read. Service name: `Claude Code-credentials`. Additional roots use `Claude Code-credentials-<sha256(path)[0..8]>`, tried for both `path` and `path + "/"` (`Core/ClaudeAccountRoots.swift:173-176`).

**Credential JSON** (`OAuthCredentialData.credential`, `:554-568`):
```json
{"claudeAiOauth":{"accessToken":"…","expiresAt":1760000000000,"subscriptionType":"max","rateLimitTier":"default_claude_max_20x"}}
```
- `expiresAt` may be a Double, Int, Int64 or numeric String. Values `<= 0` mean no expiry. A value `> 10_000_000_000` is milliseconds and is divided by 1000 (`:570-587`).
- `isExpired` is `expiresAt <= now + 60 s`. A missing `expiresAt` never expires (`:535-538`).
- `"claudeAiOauth": null` (logged out, or MCP-only) counts as "account OAuth missing". It is not a format error (`:549-552`).

**Keychain multi-account ordering** (macOS only, `:485-524`): placeholders `unknown`/`none` get priority 3, the current user name 0, an email-shaped name 1, others 2. The sort is stable. A user-cancel or auth-fail (`errSecUserCanceled` / `errSecAuthFailed`) stops immediately, so the user never gets prompt after prompt (#280).

### 3.3 Claude response model (`Core/Models.swift:206-369`)

```json
{"five_hour":{"utilization":62.0,"resets_at":"2026-…+00:00"},
 "seven_day":{…},"seven_day_opus":null,"seven_day_sonnet":null,
 "limits":[{"kind":"weekly_scoped","group":"…","percent":40,"severity":"…","resets_at":"…","is_active":true,"scope":{"model":{"display_name":"Opus"}}}]}
```
- `resets_at` is ISO-8601 with 0, 3 or 6 fractional digits. Truncate or pad the fraction to 3 digits, then retry; fall back to no fraction (`Models.swift:898-913`).
- `LimitWindow.hasNotStarted` is `utilization == 0 && resetDate == nil`. The row then shows the hint "Starts with your next message" instead of a reset.
- Window spans: `session` = 5 h (18000 s); `weekly_all` and `weekly_scoped` = 7 d (604800 s). Any other kind has no span, so no pace marker.
- `scopedLimitEntries`: when both `five_hour` and `seven_day` are null, return all of `limits`. Otherwise drop the `session` and `weekly_all` entries.
- `headlineWindow` (menu bar): `five_hour` if its utilization is non-null, else `seven_day`. Weekly-only plans (Ultra) send `five_hour: null`.
- `allUtilizations` = five_hour, seven_day, opus and sonnet utilizations, plus every scoped entry's `percent`.
- **Plan display** (`:281-317`):
  1. A non-empty `planDisplayName` wins.
  2. Otherwise start from `subscriptionType` with the first letter capitalized.
  3. The multiplier is the first `_`-split token of the tier that matches `^\d+x$`. Without one, show the base name only.
  4. If the subscription is not `max`: `"<Base> <mult>"`.
  5. For `max`: `5x` → `Max`, `20x` → `Ultra`, any other multiplier → `Max <mult>`.
  6. No `subscriptionType` → no plan row.
- **Account display**: show `email`, or `email · org` when the org name exists and does not contain the email. Without an email there is no label.

### 3.4 claude.ai session key (`Core/SessionKeyLimitsProvider.swift`)

**Request headers** (`:31-56`):
- `Cookie: sessionKey=<key>`, with automatic cookie handling off.
- `Accept: application/json`
- `User-Agent: Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15`
- `Referer: https://claude.ai`, `Origin: https://claude.ai`
- Timeout 15 s.
- Cloudflare challenges curl (`cf-mitigated: challenge`) and returns 403. The source says only URLSession gets through (`:14-15`).

**Endpoints** (`base = https://claude.ai/api`):
- `GET /organizations` returns `[{uuid, name?, capabilities?:[String]}]`. A decode error throws `credentialFormat`.
- `GET /organizations/{uuid}/usage` has the same schema as the OAuth usage response.

`mapFailure`: 200 is ok, 401 throws `sessionKeyInvalid`, 429 throws `rateLimited(retryAfter)`, anything else throws `httpStatus(n)`.

**fetch** (`:169-207`):
1. No stored key: throw `sessionKeyMissing`.
2. A cached `organizationID` exists: fetch its usage. A 403 there means "rediscover". A 401 propagates.
3. List organizations. A 403 on this list means the key is dead and throws `sessionKeyInvalid` (observed 2026-08-30).
   - Keep orgs whose `capabilities` contains `"chat"`. Orgs without the field are kept. If the filter leaves nothing, use all rows.
   - Fetch every org's usage in parallel. An org that answers `httpStatus` is skipped.
   - Restore the list order afterwards.
4. Pick the first org with `hasUsageData`, else the first org. No org: throw `sessionKeyNoOrganization`. If the pick changed, save it.
5. `hasUsageData` is true when any of five_hour, seven_day, opus or sonnet has utilization > 0 or a `resets_at`, or when any `limits[]` entry has percent > 0 or a `resets_at`.

**Key validation** (`normalize`, `:122-128`): trim whitespace and newlines. The key must start with `sk-ant-`, be 40 to 2048 characters long and contain no control characters. Otherwise throw `sessionKeyMalformed`.

**Storage**: `<stateDir>/session-key.json` holds `{"key":"…","organizationID":"…"|null}`.
- The file is created with mode 0600 first, then written atomically, then chmod 0600 again.
- Additional roots use `session-key-<pathKey>.json`, where `pathKey = sha256(standardized path)[0..8]` in hex.
- It is deliberately **not** stored in an app-owned Keychain item (an ACL prompt fires on every re-sign).

**Chain** (`:297-336`):
1. Run the primary (session key).
2. If it fails with `sessionKeyMissing`, the key is "unconfigured". Run the fallback with the caller's `allowKeychainPrompt` and rethrow the fallback's error.
3. If the key is configured, log the failure, then run the fallback with `allowKeychainPrompt=false`. If the fallback also fails, rethrow **the session-key error**, so the UI shows "re-paste key".

**Save flow** (`UsageStore.saveSessionKey`, `:1768-1790`): normalize the key, list organizations, pick (with usage, else first), save, set `configured`, refresh. Errors are shown under the field.
- Organization list in Settings: filled on first open (`refreshSessionOrganizations`). The picker shows only when there is more than one org.
- Clear: delete the file, reset the state, refresh.

### 3.5 Codex (`Core/CodexRateLimitsProvider.swift`, `Models.swift:373-463`)

Binary candidates, in order:
1. `/Applications/Codex.app/Contents/Resources/codex`
2. `~/.codex/bin/codex`
3. `/opt/homebrew/bin/codex`, `/usr/local/bin/codex`
4. ChatGPT.app bundled copies
5. The common Node tool paths
6. A login-shell PATH lookup

No binary: return nil, so the section is hidden.

Process: `codex app-server --stdio` with a 20 s timeout. Send these JSON lines on stdin and read until the response with `id:1`:
```json
{"method":"initialize","id":0,"params":{"clientInfo":{"name":"token_mac","title":"PokeTokenBar","version":"<app ver>"},"capabilities":{"experimentalApi":true}}}
{"method":"initialized","params":{}}
{"method":"account/rateLimits/read","id":1,"params":{}}
```
The result decodes as:
```
{rateLimits:{limitId?,limitName?,primary?:{usedPercent:Int,windowDurationMins?:Int,resetsAt?:Int(epoch s)},secondary?,credits?:{balance?,hasCredits,unlimited},
 individualLimit?:{limit:String,remainingPercent:Int,resetsAt:Int,used:String},planType?,rateLimitReachedType?},
 rateLimitsByLimitId?:{<id>:{…same…}}}
```
- `snapshots` = `rateLimits` plus the `rateLimitsByLimitId` entries sorted by key. Skip the key equal to `rateLimits.limitId ?? "codex"`, and any snapshot whose `limitId` equals the top-level `limitId`.
- A snapshot is visible when it has a primary, a secondary or an `individualLimit`.
- `bucketDisplayName`: `limitName ?? limitId ?? "codex"`, with `_` replaced by a space and the first letter capitalized.
- Individual limit: `usedPercent = clamp(100 - remainingPercent, 0, 100)`. It is shown with detail `"<used> / <limit>"` and has no pace marker.
- Window name (`Localization.swift:192`): 300 → "5-hour session", 10080 → "Weekly", a whole number of hours → "Nh", otherwise "Nm", nil → "Limit".
- Window class for candy: ≤ 1440 min (or nil) is a session; more is weekly.
- `maxPrimaryUsedPercent` (menu bar) is the max primary `usedPercent` over the visible buckets. The pace for that item comes from the bucket whose primary equals the max.

### 3.6 Antigravity (`Core/AntigravityRateLimitsProvider.swift`)

- Endpoints, tried in order:
  1. `$CLOUD_CODE_URL + "/v1internal:retrieveUserQuotaSummary"`, if the env var is set.
  2. `https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary`
  3. `https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary`
- Request: `POST`, body `{}`, headers `Authorization: Bearer`, `Content-Type: application/json`, `User-Agent: antigravity/2.9.1`, timeout 15 s.
- Responses: 200 decodes. 429 throws `rateLimited`. 401 or 403 throws `httpStatus` immediately. Any other status records the error and moves to the next endpoint. A transport error also moves on.
- A 401/403 overall invalidates the cache, re-acquires with `bypassCache`, and retries once if the token changed.
- Token refresh: `POST https://oauth2.googleapis.com/token`, form-encoded, timeout 10 s:
  - `client_id=1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com`
  - `client_secret=[REDACTED]` (public client secret embedded in `agy`)
  - `grant_type=refresh_token`, `refresh_token=…`
  - 429 throws `rateLimited`. 5xx throws `httpStatus`. Any other non-200 returns nil, so the old token is kept.
  - New `expiresAt = now + expires_in` (default 3600). The refresh token is kept, because Google does not rotate it.
- Token file search order (`:125-148`):
  1. `$ANTIGRAVITY_TOKEN_FILE` (tilde-expanded)
  2. `<stateDir>/antigravity-token.json`
  3. `~/.gemini/antigravity-cli/{antigravity-oauth-token,jetski-standalone-oauth-token}`
  4. `~/.gemini/antigravity-ide/…` (same two names)
  5. `~/.gemini/antigravity/…` (same two names)
  6. `~/.gemini/antigravity-oauth-token`, `~/.gemini/jetski-standalone-oauth-token`
- Token formats (`:340-390`), after an optional `go-keyring-base64:` prefix and base64 decode:
  - `{"token":{"access_token","refresh_token","expiry":"ISO"}}`
  - `{"token":"<str>"}`
  - `{"access_token","refresh_token","expiry"}`
- Expiry skew is 60 s.
- Acquisition (`:150-199`):
  1. A file credential exists. If the cache came from the same source (same access token or same refresh token), the file token is expired and the cache is not, keep the cache. Otherwise use the file credential and refresh it if it is expired or `bypassCache` is set.
  2. Else, if `!bypassCache` and the cache is valid, return the cache.
  3. On an automatic poll: return a valid cache, or refresh an expired cache that has a refresh token. Otherwise throw `keychainInteractionNotAllowed`.
  4. On a manual refresh: read the Keychain `service=gemini account=antigravity`, silently first, then with a prompt.
- If a refresh fails while the token is still valid, return the old token. If the token is expired, throw.
- Model (`Models.swift:467-590`): `{groups:[{displayName, description?, buckets:[{bucketId, displayName, window?:"5h"|"weekly", resetTime?, description?, remainingFraction}]}], description?}`.
  - A bucket without `remainingFraction` is dropped (lossy decode). It does not fail the response.
  - `usedPercent = clamp((1 - remainingFraction) * 100, 0, 100)`.
  - The bucket is 5-hour when `window == "5h"` or `bucketId` contains `5h`. It is weekly when `window == "weekly"` or `bucketId` contains `weekly`.
  - Group title: a name containing `gemini` becomes "Gemini Models". One containing `claude`, `gpt` or `3p` becomes "Claude & GPT Models". Anything else keeps its raw name.
  - Menu bar value: the max 5-hour `usedPercent` over all groups, labelled `AGY`, with no pace.

### 3.7 Cursor (`CursorRateLimitsProvider.swift`, `CursorAuthentication.swift`, `CursorUsageAPI.swift`)

- Limits request: `POST https://api2.cursor.sh/aiserver.v1.DashboardService/GetCurrentPeriodUsage`, body `{}`, timeout 10 s, ephemeral session with no cookies.
  - Headers: `Content-Type: application/json`, `Connect-Protocol-Version: 1`, `Authorization: Bearer <bearerToken(token)>`.
  - 429 throws `rateLimited(nil)`. Non-2xx throws `httpStatus` and logs the first 120 bytes of the body. A transport failure throws `httpStatus(0)`.
  - On 401/403, refresh once, then retry.
- Model: `{billingCycleStart?:String, billingCycleEnd?:String(epoch ms|s), planUsage?:{totalSpend,includedSpend,bonusSpend,remaining,limit (cents),autoPercentUsed,apiPercentUsed,totalPercentUsed}, displayMessage?}`.
  - `usedPercent = totalPercentUsed ?? includedSpend / limit * 100` (requires limit > 0).
  - Visible when `limit > 0` or `totalPercentUsed` is non-nil.
  - Epoch values: `> 1e12` is ms, `> 1e9` is seconds.
- Token sources, in order:
  1. Env or login-shell `CURSOR_SESSION_TOKEN` (not refreshable).
  2. For each root, `state.vscdb` `ItemTable` key `cursorAuth/accessToken`, with `cursorAuth/refreshToken` **from the same root**.
     - Default roots: `~/Library/Application Support/Cursor/User/globalStorage` and `…/Cursor Nightly/…`.
     - `CURSOR_DATA_DIR` overrides them. Custom scan roots are added.
     - The SQLite read retries up to 6 times with backoff `0.05 * (n+1)` s.
  - `CURSOR_USAGE_API=0` disables both the limits and the dashboard API.
- Refresh: `POST https://api2.cursor.sh/oauth/token`, body `{"grant_type":"refresh_token","client_id":"KbZUR41cY7W6zRSdpSUJ7I7mLYBKOCmB","refresh_token":…}`.
  - The response is rejected when `shouldLogout == true` or `access_token` is empty.
  - The refreshed token is kept **in memory only**, keyed by its source. If the source changes, the refreshed credential is dropped.
  - Before the result is stored, the selected source is checked again (logout race).
- `bearerToken`: percent-decode, then take what follows `::` if present.
- `sessionCookie`: if the token already has `::`, use it as is. Otherwise use `"<jwt.sub>::<jwt>"`.
- Dashboard events (usage, not limits): `POST https://cursor.com/api/dashboard/get-filtered-usage-events`.
  - Body: `{teamId:0,startDate:"<epoch ms string>",endDate,page,pageSize:100}`. ISO dates make the server return 500.
  - Headers: UA `Mozilla/5.0`, `Origin https://cursor.com`, `Referer https://cursor.com/dashboard/usage`.
  - Auth: the cookie `WorkosCursorSessionToken=<sub::jwt>` first. On 401/403, switch to Bearer.
  - Limits: max 200 pages, 120 s deadline.
  - Cache: `<stateDir>/cursor-usage-api-cache.json`, max age 6 h. Specified in the usage area.

### 3.8 OpenCode Go (`OpenCodeGoRateLimitsProvider.swift`, `Models.swift:681-742`)

- `GET https://opencode.ai/zen/go/v1/usage`, headers `Authorization: Bearer <key>` and `Accept: application/json`, timeout 10 s. 429 throws `rateLimited(nil)`. Non-2xx throws `httpStatus`.
- Key lookup for integration `opencode-go`. Root: `$OPENCODE_DATA_DIR` or `~/.local/share/opencode`.
  1. The DB is `opencode.db`, else the alphabetically first `opencode-<channel>.db`. Query: `SELECT value FROM credential WHERE integration_id = ? AND active = 1 ORDER BY time_updated DESC`. Each value is JSON `{"type":"key","key":…}`.
  2. Fallback: `auth.json[integrationID]` with `type` `"api"` or `"key"` → `key`.
- No key: return nil (not a subscriber), so the section is hidden.
- Response: `{"usage":{"rolling":{status?,percent?,resetsAt?},"weekly":{…},"monthly":{…}}}`. Each window is decoded leniently.
- A reset in the past means the window has ended: show 0 % and no reset time. The rolling row then shows the hint "Starts with your next message".
- Spans: rolling = 5 h, weekly = 7 d, monthly = none (no pace marker).
- Menu bar: `OpenCode <rolling%>` with the 5-hour pace.

### 3.9 Grok (`GrokRateLimitsProvider.swift`, `Models.swift:748-828`)

- `GET https://cli-chat-proxy.grok.com/v1/billing?format=credits`, headers `Authorization: Bearer <key>` and `Accept: application/json`, timeout 10 s.
- Token: `$GROK_HOME/auth.json`, default `~/.grok/auth.json`.
  - The file maps `"<issuer>::<user>"` to `{key, expires_at:ISO}`.
  - Take the `key` of the entry with the latest `expires_at`. A missing or invalid date counts as `distantPast`.
  - The app never refreshes this token. A 401/403 shows "Run Grok CLI once".
- Response: `{"config":{"currentPeriod":{type:"USAGE_PERIOD_TYPE_WEEKLY"|"…_MONTHLY",start,end},"creditUsagePercent":N,"productUsage":[{product,usagePercent}]}}`.
- Span is `end - start`. After `end`, utilization is 0 and no reset time is shown.
- Product names: `GrokBot` → "Grok", `GrokBuild` → "Grok Build", `Api` → "API".
- The products line lists only products with usage > 0, in server order, joined by ` · `.

### 3.10 Provider status (`Core/ProviderStatusChecker.swift`)

- `GET https://status.claude.com/api/v2/summary.json` (component `Claude Code`, id `claude_code`) and `GET https://status.openai.com/api/v2/summary.json` (component `Codex API`, id `codex`).
  - Header `User-Agent: PokeTokenBar`, timeout 10 s, both fetched in parallel.
  - The component name is matched case-insensitively.
- Component status mapping: `operational` → operational; `degraded_performance` → minor; `partial_outage` → major; `major_outage` → critical; `under_maintenance` → maintenance; anything else → unknown.
- A failed fetch is omitted and the previous value is kept. Turning the setting `statusChecksEnabled` (default true) off clears all values.
- The banner shows only on the selected provider tab, and only when the indicator is not operational.
- Dot colours: operational = green, minor and maintenance = yellow, major = orange, critical = red, unknown = gray.

### 3.11 Polling cadence (`Core/UsageStore.swift`)

- `refreshInterval`: default **120 s**, key `refreshInterval`. Presets: 0 (manual), 60, 120, 300, 900. Timer tolerance is 10 % (`:1030-1040`).
- **Limits are fetched on every refresh** (no separate minimum), after phase 1 (daily) and phase 2 (enrichment). Order (`:1262-1301`):
  1. Claude default
  2. Additional Claude accounts (serialized through a task chain)
  3. Codex
  4. Antigravity
  5. Cursor
  6. OpenCode Go
  7. Grok
  8. Provider statuses
  9. `checkLimitAlerts`
  10. `onRefresh`
- Coalescing: a refresh that starts while one is running sets `refreshPending`. Exactly one more run happens afterwards (`:1099-1113`).
- Display sleep suspends the timer and starts a probe every 300 s (tolerance 20 %). The probe resumes polling if the displays are awake. Display wake resumes polling and refreshes once.
- System wake: clear a stale suspension and refresh. Midnight (`NSCalendarDayChanged`): refresh.
- Network reconnect (`NetworkReachabilityMonitor`): only a transition from not-satisfied to satisfied counts. The first event is ignored. Cooldown is 5 s. Bundled app only.
- Empty-usage retry: when nothing was found and there were no errors, retry once after 20 s.
- Claude 429 backoff (`:1975-1994`):
  - `interval` is 300 the first time, then `min(prev * 2, 3600)`.
  - `until = now + (Retry-After ?? interval)`.
  - Automatic polls skip Claude while `now < until`.
  - Any success (automatic or manual) resets the backoff.
  - A manual refresh (`refreshLimitTokenFromKeychain`) ignores the backoff.
- Additional accounts keep their own backoff per root path, with the same formula.
- No backoff for Codex, Antigravity, Cursor, OpenCode or Grok. They are retried every poll.
- Stale: `now - updatedAt > 15 * 60` and a value exists. Applies to Claude, each additional account, Codex, Antigravity, Cursor, OpenCode and Grok.
- Global `isStale` (usage): `lastUpdated` is older than `2 * refreshInterval`, or older than 1800 s in manual mode.
- Auth-expired flags:
  - Claude: `limitsAuthExpiry = .sessionKey` when the error is `sessionKeyInvalid`. `.oauth` on `httpStatus` 401/403. Any other error leaves the flag unchanged. A success clears it.
  - Antigravity, Cursor, OpenCode and Grok: set on 401/403, cleared on success. `nil` ("no login") also clears it.
- On failure the previous value is **kept**. `limitsAvailable` becomes false only when nothing has ever loaded.

### 3.12 Pace and colour (`Core/PaceTier.swift`, `UsageStore.swift:485-490`)

- `paceFraction = (span - (resetsAt - now)) / span`. If the result is outside [0, 1] or not finite, return **nil (no clamping)**.
- `tier(utilization, pace, crit)`:
  - No pace, or `pace < 0.1`: return nil (hold early in the window).
  - `utilization >= crit`: wayOver.
  - Otherwise `delta = round(utilization - pace * 100)`:
    - `< -25` wayUnder
    - `< -10` under
    - `< 5` onPace
    - `< 15` slightlyOver
    - `< 30` over
    - else wayOver
- `gauge(...)`: use the tier when there is one. Otherwise `>= crit` → wayOver, `>= warn` → over, else onPace.
- `isCalm` = tier ≤ onPace.
- Tier colours (SwiftUI system colours): wayUnder `.blue`, under `.teal`, onPace `.green`, slightlyOver `.yellow`, over `.orange`, wayOver `.red`. The percentage text uses the same colours, except slightlyOver, which uses the primary label colour.
- Tooltip:
  - Line 1: tier name, plus ` · ` and the delta text when the delta ≠ 0 (`"N pts more used"` / `"N pts less used"`).
  - Line 2: `"Pace — an even burn across this window would sit at X% now."`, where X follows the display mode.
- Display mode: `remaining` → `max(0, 100 - u)`. It changes only the number, the fill and the marker position. Colours, alerts and the menu-bar colour still use the raw utilization.

### 3.13 Alerts (`UsageStore.swift:2091-2274`)

Window list (`buildLimitWindows`), each entry `key | name`:

- For each Claude account (prefix `claude`, or `claude.<pathKey>` for additional accounts):
  - `.fiveHour` | "Claude 5-hour session"
  - `.sevenDay` | "Claude weekly"
  - `.sevenDayOpus` | "Claude Weekly Opus"
  - `.sevenDaySonnet` | "Claude Weekly Sonnet"
  - `.scoped.<kind>.<model>.<i>` | "Claude Weekly <model>"
- Codex:
  - `codex.<bucketKey>.primary` / `.secondary` | "<Bucket> <window>"
  - `codex.<bucketKey>.individual` | "Codex personal limit"
- Antigravity: `antigravity.<gemini|3p>.<bucketId>` | "<GroupTitle> <window>".
- **Cursor, OpenCode and Grok do not alert.**

Rules:
- `tier = u >= crit ? 2 : u >= warn ? 1 : 0`.
- Tier 0 removes the key, which re-arms the window.
- Alert only when `tier > stored`. When the tier drops from 2 to 1, the stored value stays 2, so climbing back does **not** re-alert.
- With more than one Claude account, the body is suffixed ` · <account title>`.
- Notification:
  - Title: "Limit imminent" (crit) or "Limit warning".
  - Body: `"<name> at <pct>"`. In Korean: `"<name> 한도 <pct> 사용"`.
  - Sound: default for crit, none for warn.
  - Identifier: `"<key>-critical|warning"`.
- Gate: `limitNotifications` (default true) and a bundled app.
- Bubble: shown when `floatingPetEnabled && floatingPetBubbleAlerts`. It shows the single alert ranked by critical first, then highest utilization, and auto-clears after 6 s.
- Thresholds: warn default 80 (slider 50–95, step 5), crit default 95 (slider 80–100, step 5). Both sliders keep a minimum gap of 5: raising warn to ≥ crit pushes crit to warn + 5, and lowering crit to ≤ warn pushes warn to crit − 5. On load, `warn = min(stored, crit - 5)`.
- `isLimitWarning` (menu-bar warning state and companion mood) is true when any of these hold:
  - any tracked-Claude utilization ≥ crit,
  - any Codex primary, secondary or individual ≥ crit,
  - any Antigravity bucket ≥ crit,
  - the 5-hour forecast says the limit is reached before reset.
- `highestLimitUtilization` covers providers **used today** only: Claude `allUtilizations`, Codex primary and secondary (no individual), and all Antigravity buckets.
- Forecast (`:713-722`):
  - Requires utilization in [5, 100), block tokens > 0 and burn ≥ 10 000 tokens/min.
  - `minutesLeft = (100 - u) * (blockTokens / u) / burn`. The result must be under 1440, otherwise nil.
  - At u ≥ 100: depletion = now, before reset = true.

### 3.14 Menu-bar limit text (`UsageStore.swift:354-453`)

- Requires the setting `showLimitInMenu` (default **false**).
- Items, in order, only for providers with tokens today:
  - `Claude <headline%>` (Claude also requires `claude_code` used today)
  - `Codex <maxPrimary%>`
  - `AGY <max5h%>`
  - `Cursor <used%>`
  - `OpenCode <rolling%>`
  - `Grok <credit%>`
- Items are joined with ` · `. The % follows the display mode with **no suffix**.
- Percent format: one decimal, with `.0` stripped (`62%`, `62.5%`).
- Layout:
  - At most 2 lines. When there are usage items (tokens/cost/growth), they form the top line and the limit line is always the bottom line.
  - With 2 or more usage items plus limits: `[usage joined " · ", limit]`.
  - With more than 2 usage items and no limits: `[all but last joined, last]`.
- `menuLimitColorMode` (key `menuLimitColorMode`, default `gauge`):
  - `gauge`: colour every item by `PaceTier.gauge`.
  - `attention`: colour only items with a non-calm tier.
  - `off`: no colour.
- Tooltip with more than one Claude account: `trackedAccountToolTip(title)`.

---

## 4. Data and persistence

| Key / file | Type | Default | Where |
|---|---|---|---|
| `refreshInterval` | Double s | 120 | UserDefaults |
| `warnThreshold` / `critThreshold` | Double | 80 / 95 | UserDefaults |
| `showLimitInMenu` | Bool | false | UserDefaults |
| `limitDisplayMode` | `used`\|`remaining` | used | UserDefaults |
| `menuLimitColorMode` | `gauge`\|`attention`\|`off` | gauge | UserDefaults |
| `limitNotifications` | Bool | true | UserDefaults |
| `statusChecksEnabled` | Bool | true | UserDefaults |
| `floatingPetBubbleAlerts` | Bool | true | UserDefaults |
| `disableKeychainAccess` | Bool | false | UserDefaults (+ `KeychainAccessGate`) |
| `disabledProviderIDs` | [String] | [] | UserDefaults |
| `additionalClaudeConfigDirs` | String | "" | UserDefaults |
| `claudeTrackedAccount` | String | automatic | UserDefaults |
| `claudeAccountCandyArmed` | [String] | [] | UserDefaults |
| `session-key.json`, `session-key-<hash8>.json` | `{key, organizationID}` 0600 | — | `<stateDir>` |
| `antigravity-token.json` | token JSON | — | `<stateDir>` (optional, user-placed) |
| `cursor-usage-api-cache.json` | DiskCache | — | `<stateDir>` |

`<stateDir>` is `$PTB_STATE_DIR`, else `~/Library/Application Support/PokeTokenBar`.

In-memory only, lost on restart:
- token caches,
- the profile cache,
- `notifiedTier`, so the first poll after launch re-alerts every window that is already above a threshold,
- backoff state,
- statuses,
- `sessionKeyOrganizations`.

---

## 5. UI spec

Fonts: macOS sizes are `.callout` = 12 pt, `.caption` = `.caption2` = 10 pt. On Windows use Segoe UI at the same px at 100 % scaling. Optionally go 1 px larger for legibility, but keep all ratios.

System colours (light / dark):

| Role | Light | Dark |
|---|---|---|
| blue | `#007AFF` | `#0A84FF` |
| teal | `#30B0C7` | `#40C8E0` |
| green | `#28CD41` | `#32D74B` |
| yellow | `#FFCC00` | `#FFD60A` |
| orange | `#FF9500` | `#FF9F0A` |
| red | `#FF3B30` | `#FF453A` |
| gray | `#8E8E93` | `#98989D` |
| primary label | `rgba(0,0,0,.85)` | `rgba(255,255,255,.85)` |
| secondary label | `rgba(0,0,0,.50)` | `rgba(255,255,255,.55)` |
| tertiary label | `rgba(0,0,0,.26)` | `rgba(255,255,255,.25)` |
| gauge track | `rgba(0,0,0,.10)` | `rgba(255,255,255,.10)` |

### 5.1 Placement

The popover is 360 pt wide with 14 pt padding and a 12 pt scroller lane. The home scroll area is 520 pt high and laid out as a VStack with spacing 12:
1. CompanionHeader
2. Divider
3. header (today's totals and provider tabs)
4. Divider
5. `providerStatusBanner`
6. `limitsSection` + Divider, only when `selectedProviderHasLimits`

Limits are shown **for the selected provider tab only**. `selectedProviderHasLimits` (`PopoverView.swift:380-394`) per tab:
- claude_code: `!disableKeychainAccess || limits != nil || limitsAuthExpired || additionalLimits non-empty`
- codex: `codexLimits?.hasVisibleLimit`
- antigravity: `!disableKeychainAccess || hasTokenFile || hasVisibleLimit || authExpired`
- cursor, opencode and grok: `hasVisibleLimit || authExpired`

### 5.2 `limitsSection` component tree (VStack, leading, spacing 8)

1. Header HStack (spacing 4):
   - "Limits (official)", caption, secondary.
   - On Claude, when no session key is configured: a `?` button (SF `questionmark.circle`, caption, secondary). It opens a 250 pt popover (padding 12, spacing 8) with a title (caption, semibold), a body (caption2, secondary) and a right-aligned small button "Register session key" that opens Settings at the session-key field.
2. Claude, at most one notice, in this priority:
   - `sessionKeyExpiredNotice`: icon `key.slash.fill` (orange), title (caption, semibold), Spacer, small "Settings" button. Hint below (caption2, secondary). Padding 8, background orange @0.12, corner radius 8.
   - `claudeAuthExpiredNotice`: same box with icon `exclamationmark.triangle.fill` (orange). The button is "Retry", which runs a manual refresh and shows a small spinner while refreshing.
   - `claudeLimitsRefreshRow`, shown when the Keychain is not disabled and any of `claudeLimitsMissing`, `claudeLimitsStale`, `additionalLimitsPending` or `additionalLimitsStale` holds. Layout: HStack with either "Stale · <relative>" (caption, orange) or "Load official limits" (caption, secondary), a Spacer, and a small "Refresh" button (spinner while busy). Below it, the manual-refresh error (caption2, orange) when present.
3. Claude account content:
   - With more than one account, a `CapsuleTabBar`. The tracked account is marked, and each tab's help text is its account display.
   - Then for the selected account:
     - An expired, non-default account shows its notice (session key, or OAuth with `additionalAccountExpiredHint`). A stale, non-default account shows the stale badge.
     - `Plan <planDisplay>` (caption, tertiary)
     - `Account <accountDisplay>` (caption, tertiary)
     - With multiple accounts: today and month usage labels.
     - A VStack (spacing 8, opacity 0.5 when expired) of rows:
       - 5-hour session (hint "Starts with your next message")
       - the forecast row, for the tracked account only
       - Weekly
       - Weekly Opus
       - Weekly Sonnet
       - scoped entries
       - "Claude current 5h block <tokens> … Reset <relative> (<clock>)" (caption)
   - With multiple accounts and unattributed month tokens > 0: a caption2 tertiary note.
4. Codex:
   - Meta row (HStack, spacing 8, shown only when non-empty): `Plan <planType>` (caption, tertiary), "Limit reached" (caption, red) when any bucket has `rateLimitReachedType`, and the stale badge.
   - Per bucket: the bucket name (caption, secondary, top padding 2) when there is more than one bucket, then the primary row, the secondary row and the personal-spend row.
5. Antigravity:
   - An auth-expired box with a borderedProminent mini "Retry" button. Otherwise, when the Keychain is not disabled and (no value or stale), a refresh row.
   - Groups in a VStack (spacing 10). Each group has a title (caption, semibold, secondary) and its bucket rows (spacing 4).
6. Cursor:
   - Expired box.
   - Stale badge.
   - `displayMessage` (caption, secondary).
   - "Monthly included" row (reset = billing cycle end, no pace).
   - "$X remaining" (caption, tertiary).
   - "Auto usage" and "API usage" rows (no reset).
7. OpenCode: expired box, stale badge, then the 5-hour session, Weekly and Monthly rows.
8. Grok: expired box, stale badge, then a VStack (spacing 4) with the "Weekly credits / Monthly credits / Credits" row and the products line (caption, tertiary).

Expired box (generic, non-Claude): HStack with an orange triangle icon and the title (caption, medium), a hint (caption2, secondary), padding 8, background orange @0.10, corner radius 6. The rows under it are drawn at opacity 0.5.

Auth-expired copy:

| Provider | Title | Hint |
|---|---|---|
| Grok | "Grok login expired — limits can't refresh" | "Run Grok CLI once to refresh the login, or run grok login." |
| OpenCode | `openCodeGoAuthExpiredTitle` | "…/connect…" |
| Cursor | `cursorAuthExpiredTitle` | `cursorAuthExpiredHint` |
| Antigravity | `antigravityAuthExpiredTitle` | `antigravityAuthExpiredHint` |

Port the full 8-language strings from `Core/Localization.swift`.

### 5.3 `quotaRow` (`PopoverView.swift:758-796`), the single row used by every provider

```
VStack(spacing 2)
  HStack: name [callout] · Spacer · detail? [caption, monospaced, secondary]
          · reset label [caption, tertiary] | idleHint [caption, tertiary] · percent [callout, monospaced, tier.percentColor]
  LimitProgressBar
tooltip = paceHelp
```
- Percent text: `TokenFormatter.percent(display)`. In remaining mode it gets a suffix: "62% left", "62% 남음", "残り62%".
- Reset label: a relative countdown, then ` (HH:mm)` when the reset is today or within 6 h, else ` (EEEE d HH:mm)`.
  - Use the app-language locale and 24-hour time.
  - The screenshot shows `1 hr, 59 min (16:57)` and `2 days, 23 hr (Thursday 1, 14:57)`.
- `LimitProgressBar`:
  - A small linear progress bar. The visible track is **6 pt** high and full width. Fill = `clamp(display, 0, 100)` in the tier colour. The bar sits in a 12 pt layout cell.
  - Pace marker: a rounded rect **2.5 × 10 pt** (track 6 + 2 pt overhang each side), corner radius 1.25, fill primary @0.55. x = `(width - 2.5) * markerFraction`, where `markerFraction = displayPercent(pace * 100) / 100`. It is vertically centred and not hit-testable.
  - The marker moves only on re-render (refresh or reopen). There is no dedicated timer.
- Screenshots: `assets/screenshot-pace-marker.png` (dark, used mode, with marker), `assets/screenshot-quota-alignment.png` (remaining mode, green tier), `assets/screenshot-antigravity-limits.png` (an older layout with the reset after the %; the current code puts the reset **before** the %).

### 5.4 Status banner

HStack (spacing 6): a 7 × 7 circle in the indicator colour, the label (caption, medium), the component name (caption, secondary, 1 line), and a Spacer.

### 5.5 Forecast row

HStack (spacing 4, leading padding 2):
- Icon `exclamationmark.triangle.fill` or `checkmark.circle` (caption2).
- Text "At current rate, limit hit at HH:mm" or "Won't hit limit before reset at current rate" (caption).
- Colour: orange when the limit is reached before reset, tertiary otherwise.

### 5.6 Settings

- Refresh interval picker (Manual, 1, 2, 5 or 15 min).
- Limit display segmented control (Used / Remaining).
- Menu-bar limit colour picker (gauge / attention / off), shown when the limit is shown in the menu bar.
- Notifications group:
  - "Limit alerts" toggle.
  - When on: "Warning" slider (50–95, step 5) and "Critical" slider (80–100, step 5). Each has its value at the right (caption, monospaced, 38 pt wide).
  - Status checks toggle with a hint (caption2, tertiary).
- Advanced → session key (`SettingsView.swift:551-626`):
  - Row 1: the label "claude.ai session key" plus a badge: "✓ Saved" (caption2, green) or "Expired" (caption2, orange). Below it, a hint (caption2, tertiary). With extra accounts, `sessionKeyPerAccountNote` is appended.
  - Row 2: a SecureField (max 220 pt, rounded border). The placeholder is `••••••••` when a key is saved, else `sk-ant-sid…`. The stored value is **never echoed back**.
    - "Save" button (spinner while validating; disabled when the field is empty or validating). Enter submits.
    - "Delete" button, shown when a key is configured.
  - Organization picker (max 220 pt), shown only when there is more than one org.
  - Below: the error (caption2, orange), or the storage note (caption2, tertiary).
  - Per-account key rows follow the same pattern.
- Screenshot: `assets/screenshot-session-key.png`.

---

## 6. Windows adaptation

| macOS mechanism | Windows / Tauri equivalent |
|---|---|
| Claude Code token in Keychain (`Claude Code-credentials[-hash8]`) | Claude Code on Windows writes `%USERPROFILE%\.claude\.credentials.json` (plaintext). It is the only source, so drop every Keychain branch. The automatic path becomes: read the file each poll and keep the in-memory cache as a fallback. "Manual refresh" means re-read the file with `bypassCache` and ignore the backoff. `credentialMissingAccountOAuth` stays valid. The `keychainInteractionNotAllowed` case becomes "no credential" (`no_credentials`). |
| Extra roots' Keychain services | `<root>\.credentials.json` only. |
| `disableKeychainAccess` toggle, `KeychainAccessGate`, `KeychainNoUIQuery`, prompt-cancel (#280) logic | Not needed. Omit the setting, or keep a hidden `false`. All `!disableKeychainAccess` UI gates evaluate to true. |
| Antigravity Keychain `service=gemini, account=antigravity` (go-keyring) | go-keyring on Windows uses Credential Manager, generic credential target `gemini:antigravity`. Read it with `CredReadW` (crate `windows` or `keyring` v3 with `Entry::new_with_target`). No prompt exists on Windows, so it is safe on **automatic** polls too. Strip the `go-keyring-base64:` prefix if present. Token files: `%USERPROFILE%\.gemini\…` with the same relative names. |
| Session key file (0600 in App Support) | Store it in **Windows Credential Manager** (`keyring` crate, service `PokeTokenBar`, user `session-key` / `session-key-<hash8>`). It never prompts on Windows, so the macOS reason for avoiding the keychain does not apply. Alternative: a file in `app_data_dir()` encrypted with DPAPI `CryptProtectData`. Do not write plaintext JSON. Keep the JSON shape `{key, organizationID}`. |
| URLSession passing Cloudflare on claude.ai | `ureq` (rustls) has a non-browser TLS fingerprint and may get `cf-mitigated: challenge` 403. Options, in order: `reqwest` with `native-tls` (SChannel, the same stack as Edge). Last resort: issue the fetch from a hidden WebView2 window. **Verify before shipping** (see §8). Change the UA to the Windows Edge string, e.g. `Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0`. |
| `codex app-server --stdio` via `/Applications/...` paths | Resolve in this order: `where codex`, `%APPDATA%\npm\codex.cmd`, `%LOCALAPPDATA%\Programs\codex\codex.exe`, `%USERPROFILE%\.codex\bin\codex.exe`. A `.cmd` shim must run through `cmd /C`. Spawn with `CREATE_NO_WINDOW` (0x08000000), use the same 20 s timeout, and kill the process tree on timeout. Keep the existing rollout-log reader as the fallback when no binary is found. |
| Cursor `~/Library/Application Support/Cursor[ Nightly]/User/globalStorage/state.vscdb` | `%APPDATA%\Cursor\User\globalStorage\state.vscdb` and `%APPDATA%\Cursor Nightly\User\globalStorage\state.vscdb`. Open read-only (`rusqlite`, `SQLITE_OPEN_READ_ONLY`, `immutable=1` is not safe because Cursor writes WAL), with the same 6-try busy retry. |
| OpenCode `~/.local/share/opencode` | OpenCode uses XDG paths on Windows too: `%USERPROFILE%\.local\share\opencode`. Honour `OPENCODE_DATA_DIR`. |
| Grok `~/.grok/auth.json` | `%USERPROFILE%\.grok\auth.json`. Honour `GROK_HOME`. |
| Env from the login shell (`UsageEnvironment`) | Process env, plus `HKCU\Environment` (a GUI app launched before a `setx` does not inherit the change). |
| `NSStatusItem` title text with coloured runs | The Windows tray cannot show text. Two options: (a) render the headline % into the tray icon (a 16 × 16 / 32 × 32 RGBA badge drawn in the tier colour; draw with `image` + `ab_glyph` and call `tray.set_icon`), and (b) put the full `Claude 62% · Codex 30%` line in the tooltip, which is limited to **127 chars** (NOTIFYICONDATA `szTip`). Keep `set_title` for macOS builds only. |
| `UNUserNotificationCenter` (title/body/sound/identifier) | `tauri-plugin-notification`. Windows toast: title and body as above. For sound, use the toast default audio on crit and silent audio on warn. If the plugin supports tags, use the identifier `"<key>-critical|warning"` as the toast tag so a repeat replaces the old toast. No permission prompt is needed; skip `requestNotificationAuthorizationIfNeeded`. |
| `NWPathMonitor` reconnect | WinRT `NetworkInformation::NetworkStatusChanged`. In the handler, check `GetInternetConnectionProfile()?.GetNetworkConnectivityLevel() == InternetAccess`. Same transition rule (ignore the first event) and 5 s cooldown. |
| `screensDidSleep` / `screensDidWake` | `RegisterPowerSettingNotification(GUID_CONSOLE_DISPLAY_STATE)` on a hidden window. `WM_POWERBROADCAST` / `PBT_POWERSETTINGCHANGE` gives data 0 = off, 1 = on, 2 = dimmed (treat dimmed as on). Keep the 300 s probe. |
| `NSWorkspace.didWake` | `WM_POWERBROADCAST` with `PBT_APMRESUMEAUTOMATIC`. |
| `NSCalendarDayChanged` | In the scheduler tick (already 5 s), compare the local date to the last refresh date. |
| App Nap activity | Not applicable. |
| NSPopover `?` help popover | An inline expandable panel or a small absolutely positioned card inside the webview. |
| `Text(date, style: .relative)` | Use `Intl.RelativeTimeFormat` / `Intl.DateTimeFormat` with the app language, refreshed on render. Clock suffix: `{hour:'2-digit',minute:'2-digit',hourCycle:'h23'}`, plus `weekday:'long', day:'numeric'` when far. |
| SF Symbols | Inline SVG: warning triangle, key-slash, question-circle, check-circle. |

---

## 7. Gaps vs the current Windows code

`limits.rs` and `lib.rs` as of v0.2.3:

**Claude**
1. `limits[]` is not parsed, so `weekly_scoped` per-model rows are missing.
2. A weekly-only plan works, but there is no headline concept.
3. No profile call, so no account label, no `planDisplayName` and no plan row (`subscriptionType`/`rateLimitTier` are ignored).
4. Expiry check:
   - Only an `i64` ms value is supported.
   - There is no 60 s skew.
   - A string or seconds value is mis-read.
   - `"claudeAiOauth": null` returns `no_credentials` instead of a reauth (`/login`) message.
5. No in-memory cache, and no retry after 401/403 with a re-read of the file.
6. No `CLAUDE_CONFIG_DIR` support for the default root, and no additional accounts or tabs.
7. No session-key path at all: no Settings UI, no organization logic, no chain.
8. No 429 backoff:
   - `Retry-After` is ignored.
   - Polling continues at 5-minute intervals.
   - The `MIN_LIMITS_INTERVAL` of 5 min differs from macOS, which polls every refresh (default 2 min). Pick one: macOS parity is "every refresh plus exponential backoff on 429".
9. **On error, `windows` becomes empty**, so the bars vanish on any transient failure. macOS keeps the last value and adds a stale badge after 15 min.
10. No `updatedAt` or stale badge, and no "Load official limits / Refresh" row. The Windows manual refresh refreshes everything.
11. The error text is a raw key mapped through `limitErrors`, which has only 4 keys. Missing: the `friendlyLimitError` copy, including the "If you only use Codex you can ignore this" note.

**Codex**

12. Reads only the rollout logs (`payload.rate_limits.primary/secondary`, `used_percent`, `window_minutes`, `resets_at`). Missing:
    - multi-bucket (`rateLimitsByLimitId`),
    - the personal spend limit,
    - credits,
    - `planType`, `rateLimitReachedType` ("Limit reached"),
    - live data when Codex has not run recently.
    The log approach zeroes expired windows, which macOS does not do for Codex. Keep that behaviour only for the log fallback.
13. The window key naming differs: Windows `7d` vs macOS `10080 → "Weekly"`, and a generic `Nh`/`Nm`. The UI maps `5h` and `7d` only.

**Missing providers**

14. Antigravity, Cursor, OpenCode Go and Grok limits are absent.
15. Provider status (statuspage) is absent.

**Display**

16. No pace marker, no `PaceTier` colours or tooltip, and no reset clock suffix:
    - `resetIn` gives "resets in 2d 3h" only.
    - The bar is 8 px instead of 6 pt.
    - Tone uses the **hardcoded 80/95** instead of `warn_threshold`/`crit_threshold`.
    - `--accent #2da44e` is used instead of the six-tier palette.
    - The percent shows `Math.round`, not one decimal with `.0` stripped.
    - The remaining suffix is placed before the number ("Remaining 38%") instead of after it ("38% left").
17. The limits card shows every provider at once. macOS shows only the selected provider tab's limits.
18. No `hasNotStarted` hint, no forecast row, no "Claude current 5h block" row.
19. Tray:
    - `peak` takes the max over **all** windows of **all** providers, including providers not used today.
    - It uses a single label `limit NN%` instead of per-provider items, so there is no headline rule.
    - There is no colour.
    - Windows shows only the tooltip, and there is no icon badge.

**Alerts**

20. Alerts:
    - `alerted.insert(key, tier)` stores lower tiers. A 2 → 1 → 2 swing re-alerts critical, while macOS keeps 2 until the value drops below warn.
    - The key is `provider.windowKey` with no account prefix.
    - The body is unlocalized: `"claude 5h 82%"`. macOS: `"Claude 5-hour session at 82%"`.
    - The English crit title differs: "Limit almost reached" vs "Limit imminent".
    - No crit-only sound, no floating-pet bubble alert, no tag-based replacement.
21. Threshold sanitize only clamps `crit >= warn`. The macOS 5-point gap push is missing, both in the UI and on load.

**Polling and settings**

22. Polling:
    - Default refresh is 1 min. macOS default is 2 min.
    - The preset list should be `[0,1,2,5,15]`.
    - No coalescing of overlapping refreshes: `refresh_limits` can run on the scheduler thread and from a command at the same time.
    - No display-sleep suspend, no wake refresh, no network-reconnect refresh, no midnight trigger.
23. No per-provider enable switch gating the limits.
24. No `menuLimitColorMode`, `statusChecksEnabled` or `floatingPetBubbleAlerts` settings.

---

## 8. Open questions

1. Does the claude.ai `/api/organizations` call pass Cloudflare from a Windows `reqwest`/SChannel client? If not, route it through WebView2. Probe with a real key before building the UI.
2. What exactly is the go-keyring Windows target name for Antigravity? The expected value is `gemini:antigravity` as a generic credential. Confirm with `cmdkey /list` on a machine where `agy` is logged in, and check whether the base64 prefix is used.
3. Is `codex app-server` available on Windows in the same CLI versions? Does the npm `.cmd` shim forward stdio correctly? Otherwise invoke `node <path>\codex.js` directly.
4. Should the Windows build keep a 5-minute floor on limit polling (it protects against 429 without backoff)? Or should it switch to the macOS model (every refresh plus 300 → 3600 s backoff)? This spec recommends the macOS model.
5. Tray: should it show a %-badge icon or keep the Pokémon sprite icon with the tooltip only? A badge competes with the sprite animation owned by the tray/menubar area.
6. Additional Claude accounts: decide whether they ship in the same milestone. The pipeline here assumes they do, but discovery belongs to another spec.
7. On Windows, should Claude Code's credentials file be preferred when `CLAUDE_CONFIG_DIR` is set system-wide? macOS skips the "reauth" diagnosis in that case; Windows should read `%CLAUDE_CONFIG_DIR%\.credentials.json` directly.
