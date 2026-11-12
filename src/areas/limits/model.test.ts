import assert from "node:assert/strict";
import { pick } from "../../kit/i18n";
import type { LimitWindow, ProviderLimits } from "../../shared";
import {
  codexWindowName, forecast, friendlyError, paceHelp, percentText, resetClock, rowLook, rowName,
  sectionEntries, setCrit, setWarn, windowPace,
} from "./model";
import { limitsStrings } from "./strings";

const en = pick(limitsStrings, "en");
const ko = pick(limitsStrings, "ko");
const ja = pick(limitsStrings, "ja");

const win = (o: Partial<LimitWindow>): LimitWindow => ({
  key: "5h", label: "fiveHour", arg: null, group: null, used_percent: 0, resets_at: null, span_secs: null,
  detail: null, not_started: false, expired: false, ...o,
});

// Percent text follows the display mode and puts "left" after the number.
assert.equal(percentText(62, "used", en), "62%");
assert.equal(percentText(37.5, "remaining", en), "62.5% left");
assert.equal(percentText(62, "remaining", ko), "38% 남음");
assert.equal(percentText(62, "remaining", ja), "残り38%");
assert.equal(percentText(120, "remaining", en), "0% left");

// Reset clock: short when near, weekday + day when far; always 24-hour.
const now = new Date(2026, 9, 5, 10, 0).getTime();
assert.equal(resetClock(new Date(2026, 9, 5, 16, 57).getTime(), now, "en"), " (16:57)");
const late = new Date(2026, 9, 5, 22, 0).getTime();
assert.equal(resetClock(new Date(2026, 9, 6, 2, 0).getTime(), late, "en"), " (02:00)");
assert.equal(resetClock(new Date(2026, 9, 6, 17, 0).getTime(), now, "en"), " (Tuesday 6, 17:00)");
assert.equal(resetClock(new Date(2026, 9, 8, 14, 57).getTime(), now, "ko"), " (8일 목요일 14:57)");

// Pace: marker mirrors in remaining mode, tooltip names the tier and the delta.
const reset = now + 2.5 * 3600_000;
const w = win({ used_percent: 80, resets_at: new Date(reset).toISOString(), span_secs: 18000 });
const pace = windowPace(w, now);
assert.equal(pace, 0.5);
const look = rowLook(75, pace, 80, 95, "remaining");
assert.equal(look.gauge, "over");
assert.equal(look.marker, 0.5);
assert.equal(paceHelp(look, 75, "used", en), "Fast · 25 pts more used\nPace — an even burn across this window would sit at 50% now.");
assert.equal(paceHelp(rowLook(50, 0.5, 80, 95, "used"), 50, "used", en), "On pace\nPace — an even burn across this window would sit at 50% now.");
assert.equal(paceHelp(rowLook(1, 0.05, 80, 95, "used"), 1, "used", en), "Pace — an even burn across this window would sit at 5% now.");
assert.equal(paceHelp(rowLook(1, null, 80, 95, "used"), 1, "used", en), null);
assert.equal(windowPace(win({ resets_at: new Date(now + 6 * 3600_000).toISOString(), span_secs: 18000 }), now), null);

// Names.
assert.equal(codexWindowName("300", en), "5-hour session");
assert.equal(codexWindowName("10080", ko), "주간");
assert.equal(codexWindowName("120", en), "2h");
assert.equal(codexWindowName("45", en), "45m");
assert.equal(codexWindowName(null, en), "Limit");
assert.equal(rowName(win({ label: "scoped", arg: "Opus" }), en), "Weekly Opus");
assert.equal(rowName(win({ label: "scoped" }), en), "Weekly (scoped)");
assert.equal(rowName(win({ label: "credits", arg: "monthly" }), en), "Monthly credits");

// Errors.
assert.match(friendlyError("http_401", en)!, /If you only use Codex/);
assert.equal(friendlyError("http_500", en), "Failed to fetch Claude limits (500).");
assert.equal(friendlyError("rate_limited", en), en.limitRefreshRateLimited);
assert.equal(friendlyError("network", en), en.limitRefreshGeneric);
assert.equal(friendlyError(null, en), null);

// Which tabs get a section.
const p = (provider: string, o: Partial<ProviderLimits> = {}): ProviderLimits => ({
  provider, account: null, windows: [], error: null, updated_at: null, auth_expired: null, plan: null,
  account_label: null, limit_reached: false, message: null, remaining_usd: null, products: [], ...o,
});
const all = [p("claude_code"), p("codex"), p("antigravity"), p("grok", { auth_expired: "token" }), p("cursor")];
assert.equal(sectionEntries(all, "claude_code").length, 1);
assert.equal(sectionEntries(all, "codex").length, 0);
assert.equal(sectionEntries(all, "antigravity").length, 1);
assert.equal(sectionEntries(all, "grok").length, 1);
assert.equal(sectionEntries(all, "cursor").length, 0);

// Forecast.
const five = win({ used_percent: 50, resets_at: new Date(now + 3600_000).toISOString() });
const block = { start: "", end: "", total: 1_000_000, tokens_per_minute: 20_000 };
assert.deepEqual(forecast(five, block, now), { at: now + 50 * 60_000, beforeReset: true });
assert.equal(forecast(five, { ...block, tokens_per_minute: 5_000 }, now), null);
assert.deepEqual(forecast(win({ used_percent: 100, resets_at: five.resets_at }), null, now), { at: now, beforeReset: true });

// Threshold coupling keeps a 5-point gap.
assert.deepEqual(setWarn(95, 95), { warn_threshold: 95, crit_threshold: 100 });
assert.deepEqual(setWarn(70, 95), { warn_threshold: 70, crit_threshold: 95 });
assert.deepEqual(setCrit(85, 80), { warn_threshold: 75, crit_threshold: 80 });

// Every language has every key.
for (const lang of ["ko", "ja", "es", "fr", "pt", "de", "ru"] as const) {
  const missing = Object.keys(limitsStrings.en).filter((k) => !(k in (limitsStrings[lang] ?? {})));
  assert.deepEqual(missing, [], lang);
}
