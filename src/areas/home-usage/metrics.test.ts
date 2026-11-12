// Run with `npm test`. Plain asserts, no framework.
import assert from "node:assert/strict";
import type { DailyLedger, ProviderSnapshot } from "../../shared";
import {
  axisLabel, barHeight, columns, currentStreak, dailyTrendTotals, Day, heatmapDays, level, litBlocks,
  makeRecap, monthDailyTotals, ordered, period, providerColor, recentGrid, recordedSince, segments,
  shifted, showsPeak, trendProviders,
} from "./metrics";

const ledger = (days: Record<string, number>, since: string | null = null): DailyLedger => ({
  tokens_by_day: days,
  covered_since: since,
});
const day = (date: string, total: number): Day => ({
  date, total, cost: 0, coverage: { reported: false, estimated: false, unknown: false },
});

// ---- bar geometry ----
assert.equal(barHeight(0, 100), 1.5);
assert.equal(barHeight(5, 0), 1.5);
assert.equal(barHeight(1, 1000), 1.5, "tiny day keeps the baseline");
assert.equal(barHeight(50, 100), 13);
assert.equal(barHeight(200, 100), 26, "clamped to the track");
assert.equal(showsPeak([{ tokens: 1 }, { tokens: 0 }]), false);
assert.equal(showsPeak([{ tokens: 1 }, { tokens: 2 }]), true);

// ---- columns ----
const month = columns([day("2026-11-05", 9), day("2026-10-31", 4)], "2026-11-05", "month");
assert.equal(month.length, 30);
assert.equal(month[4].tokens, 9);
assert.equal(month[5].isFuture, true);
assert.ok(!month.some((c) => c.date.startsWith("2026-10")), "other months never appear");
const rolling = columns([day("2026-10-31", 4)], "2026-11-05", "14d");
assert.equal(rolling.length, 14);
assert.equal(rolling[0].date, "2026-10-23");
assert.equal(rolling[13].date, "2026-11-05");
assert.equal(rolling.find((c) => c.date === "2026-10-31")?.tokens, 4);
assert.equal(columns([], "2024-03-01", "7d")[0].date, "2024-02-24", "leap year arithmetic");

// ---- axis labels ----
assert.equal(axisLabel("2026-11-05", "2026-11-05", "month"), "5");
assert.equal(axisLabel("2026-11-07", "2026-11-05", "month"), null, "too close to today");
assert.equal(axisLabel("2026-11-14", "2026-11-05", "month"), "14");
assert.equal(axisLabel("2026-11-01", "2026-11-05", "month"), "1");
assert.equal(axisLabel("2026-11-03", "2026-11-05", "month"), null);
// 14-day window ending Thu 11/5 (matches screenshot-home.png: 29, 1, 5).
const labels14 = columns([], "2026-11-05", "14d").map((c) => axisLabel(c.date, "2026-11-05", "14d")).filter(Boolean);
assert.deepEqual(labels14, ["29", "1", "5"]);
// 7-day window: the midpoint (diff 3) is labelled.
const labels7 = columns([], "2026-11-20", "7d").map((c) => axisLabel(c.date, "2026-11-20", "7d")).filter(Boolean);
assert.deepEqual(labels7, ["17", "20"]);
assert.equal(axisLabel("2026-11-01", "2026-11-02", "7d"), null, "1st too close to today");
assert.equal(axisLabel("2026-11-01", "2026-11-07", "7d"), "1", "diff 6, far from midpoint");
assert.equal(axisLabel("2026-11-01", "2026-11-05", "7d"), null, "diff 4, near midpoint");

// ---- stacking ----
const series = (id: string, pts: [string, number][]) => ({ id, name: id, days: pts.map(([d, t]) => day(d, t)), reportsCost: true });
const stack = ordered([series("codex", [["d1", 5]]), series("claude_code", [["d1", 5]]), series("gemini", [["d1", 0]]), series("pi", [["d1", 9]])]);
assert.deepEqual(stack.map((s) => s.id), ["pi", "claude_code", "codex"], "desc total, ties by id, zero dropped");
const segs = segments("d1", stack, 26);
assert.equal(segs.reduce((n, s) => n + s.height, 0), 26);
assert.equal(segs[0].id, "pi");
assert.deepEqual(segments("d2", stack, 26), []);
assert.equal(providerColor("claude_code"), "#D97857");
assert.equal(providerColor("codex"), "#5C9EF2");
assert.equal(providerColor("unknown"), "#D97857");

// ---- store series ----
const snap = (id: string, month: [string, number][], recent: [string, number][] = [], reportsCost = true): ProviderSnapshot => ({
  provider_id: id, display_name: id, today: null, active_block: null, week_total: null, month_total: null,
  month_daily: month.map(([date, total]) => ({ date, total, cost: total / 1000 })),
  recent_daily: recent.map(([date, total]) => ({ date, total, cost: total / 1000 })),
  last_usage: null, reports_cost: reportsCost,
});
const snaps = [snap("a", [["2026-11-01", 1000], ["2026-11-02", 2000]], [["2026-10-31", 500], ["2026-11-01", 1000], ["2026-11-02", 2000]]), snap("b", [["2026-11-02", 3000]], [], false)];
const totals = monthDailyTotals(snaps);
assert.deepEqual(totals.map((d) => [d.date, d.total, d.cost]), [["2026-11-01", 1000, 1], ["2026-11-02", 5000, 2]], "cost only from reportsCost");
assert.deepEqual(dailyTrendTotals(snaps, "7d").map((d) => d.date), ["2026-10-31", "2026-11-01", "2026-11-02"]);
assert.deepEqual(dailyTrendTotals(snaps, "7d").find((d) => d.date === "2026-11-02")?.total, 5000, "falls back to month series");
assert.equal(trendProviders(snaps, "7d")[0].days.length, 3);

// ---- heatmap ----
assert.deepEqual([0, 1, 25, 26, 50, 75, 76, 100].map((t) => level(t, 100)), [0, 1, 1, 2, 2, 3, 4, 4]);
const grid = recentGrid({ "2026-11-05": 10 }, "2026-11-01", "2026-11-05", 0);
assert.equal(grid.length, 16);
assert.ok(grid.every((c) => c.length === 7));
assert.equal(grid[15][0].date, "2026-11-01", "Sunday-first week");
assert.equal(grid[15][4].tokens, 10);
assert.equal(grid[15][5].isOutsideRange, true, "future days");
assert.equal(grid[14][6].isOutsideRange, true, "before recording");
assert.equal(grid[0][0].date, "2026-07-19");
assert.equal(recentGrid({}, null, "2026-11-05", 1)[15][0].date, "2026-11-02", "Monday-first week");
assert.deepEqual(heatmapDays(ledger({ a: 5, b: 9 }), [day("b", 3), day("c", 4)]), { a: 5, b: 9, c: 4 });
assert.equal(recordedSince(ledger({}, "2026-10-01"), [day("2026-11-01", 0)]), "2026-10-01");
assert.equal(recordedSince(ledger({}), []), null);

// ---- streak ----
assert.deepEqual(currentStreak(ledger({ "2026-11-04": 1, "2026-11-05": 1, "2026-11-03": 1 }), "2026-11-05"), { days: 3, includesToday: true });
assert.deepEqual(currentStreak(ledger({ "2026-11-04": 1, "2026-11-03": 1 }), "2026-11-05"), { days: 2, includesToday: false });
assert.deepEqual(currentStreak(ledger({ "2026-02-28": 1, "2026-03-01": 1 }), "2026-03-01"), { days: 2, includesToday: true }, "month boundary");
assert.deepEqual(currentStreak(ledger({}), "2026-11-05"), { days: 0, includesToday: false });

// ---- recap ----
const now = new Date(2026, 10, 5); // Thu Nov 5 2026
const wk = period("week", now, 0);
assert.equal(wk.start.getDate(), 1);
assert.equal(shifted(wk, -1, 0).start.getDate(), 25);
assert.equal(shifted(period("month", now, 0), -1, 0).start.getMonth(), 9);
assert.equal(period("year", now, 0).end.getFullYear(), 2027);
const l = ledger({ "2026-10-25": 100, "2026-10-26": 300, "2026-11-01": 200, "2026-11-02": 200, "2026-11-04": 50 }, "2026-10-20");
const dex = [
  { caught_at: new Date(2026, 10, 2, 12).toISOString(), id: 1 },
  { caught_at: new Date(2026, 10, 3, 12).toISOString(), id: 2 },
  { caught_at: new Date(2026, 10, 3, 12).toISOString(), id: 3, released_at: "x" },
  { caught_at: new Date(2026, 9, 3).toISOString(), id: 4 },
];
const r = makeRecap(l, dex, wk, "2026-11-05", 0);
assert.equal(r.buckets.length, 7);
assert.equal(r.total, 450);
assert.equal(r.activeDays, 3);
assert.equal(r.countedDays, 5);
assert.equal(r.bestStreak, 2);
assert.deepEqual(r.bestDay, { key: "2026-11-02", tokens: 200 }, "last maximal on ties");
assert.equal(r.previousTotal, 400, "same 5 elapsed days of last week");
assert.equal(r.delta, 50 / 400);
assert.equal(r.isInProgress, true);
assert.equal(r.canGoBack, true);
assert.deepEqual(r.graduated.map((e) => e.id), [2, 1], "newest first, released excluded");
assert.equal(r.buckets[4].isCurrent, true);
assert.equal(r.buckets[5].hasData, false, "future");
const prev = makeRecap(l, dex, shifted(wk, -1, 0), "2026-11-05", 0);
assert.equal(prev.previousTotal, null, "ledger does not cover the week before");
assert.equal(prev.canGoBack, true, "last day of the week before is covered");
assert.equal(prev.isInProgress, false);
const yr = makeRecap(l, [], period("year", now, 0), "2026-11-05", 0);
assert.equal(yr.buckets.length, 12);
assert.equal(yr.buckets[9].tokens, 400);
assert.equal(yr.buckets[10].isCurrent, true);
assert.equal(yr.buckets[8].hasData, false);
assert.equal(litBlocks(0, 10), 0);
assert.equal(litBlocks(1, 1000), 1);
assert.equal(litBlocks(500, 1000), 4);
