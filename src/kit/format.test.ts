// Run with `npm test`. Plain asserts, no framework.
import assert from "node:assert/strict";
import { compact, cost, costCompact, costText, percent } from "./format";
import { gauge, paceFraction, tier } from "./pace";

const compactCases: [number, string][] = [
  [987, "987"], [12_345, "12.3K"], [1_000, "1K"], [999_949, "999.9K"], [999_950, "1M"],
  [190_612_940, "190.6M"], [1_240_000_000, "1.24B"], [2_000_000_000, "2B"], [-12_345, "-12.3K"],
];
for (const [v, want] of compactCases) assert.equal(compact(v), want, String(v));

assert.equal(percent(79.96), "80%");
assert.equal(percent(88.35), "88.3%");
assert.equal(cost(2.756), "$2.76");
assert.deepEqual([9.5, 99.96, 311, 12_345].map(costCompact), ["$9.5", "$100", "$311", "$12.3K"]);
const unknownOnly = { reported: false, estimated: false, unknown: true };
assert.equal(costText(1, unknownOnly, true, "Unavailable"), "$—");
assert.equal(costText(1, unknownOnly, false, "Unavailable"), "Unavailable");

assert.equal(tier(50, 0.05, 95), null);
assert.equal(tier(50, 0.5, 95), "onPace");
assert.equal(tier(96, 0.5, 95), "wayOver");
assert.equal(tier(20, 0.5, 95), "wayUnder");
assert.equal(gauge(85, null, 80, 95), "over");
assert.equal(paceFraction(1_000, 2_000, 0), 0.5);
assert.equal(paceFraction(3_000, 2_000, 0), null);

console.log("kit/format.test.ts ok");
