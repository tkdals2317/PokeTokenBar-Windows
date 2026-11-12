import assert from "node:assert/strict";
import { pick } from "../../kit/i18n";
import {
  PettingDetector, agentTitle, bubbleContent, capFrameRate, fittedBody, frameFloor, heartTop, hoverText,
  panelSize, pickBubble, prayerEmojiSize, prayerHoverLine, prayerQuipIndex, strokeDistance,
} from "./petLogic";
import { josa, petStrings } from "./strings";

const en = pick(petStrings, "en");
const ko = pick(petStrings, "ko");
const g = (n: number) => new Intl.NumberFormat(undefined).format(n);

// Hover text: tokens, limit (used / remaining), sulk hours → days, event line, prayers.
const base = { todayTokens: 12_400_000, limitUtilization: null, mode: "used" as const, sulkIdle: null, eventLine: null, prayers: 0 };
assert.equal(hoverText(en, base), `Today: ${g(12_400_000)} tokens`);
assert.equal(hoverText(en, { ...base, limitUtilization: 42 }), `Today: ${g(12_400_000)} tokens (limit 42%)`);
assert.equal(hoverText(en, { ...base, limitUtilization: 42, mode: "remaining" }), `Today: ${g(12_400_000)} tokens (limit 58% left)`);
assert.equal(hoverText(en, { ...base, sulkIdle: 3 * 3600 + 59 }), `Today: ${g(12_400_000)} tokens · Sulking for 3 hours`);
assert.equal(hoverText(en, { ...base, sulkIdle: 47 * 3600 }).endsWith("Sulking for 47 hours"), true);
assert.equal(hoverText(en, { ...base, sulkIdle: 49 * 3600 }).endsWith("Sulking for 2 days"), true);
assert.equal(hoverText(en, { ...base, eventLine: "Pikachu hatched from the egg!", prayers: 1 }),
  `Today: ${g(12_400_000)} tokens\nPikachu hatched from the egg!\n1 prayer · just trying it out`);

// Prayer quips step every 30 and cap at the 11th.
assert.deepEqual([0, 1, 29, 30, 299, 300, 9999].map(prayerQuipIndex), [null, 0, 0, 1, 9, 10, 10]);
assert.equal(prayerHoverLine(en, 0), "");
assert.equal(prayerHoverLine(en, 31), "31 prayers · heaven hasn't checked yet");

// Korean particles.
assert.equal(josa("피카츄", "이", "가"), "피카츄가");
assert.equal(josa("이상해꽃", "이", "가"), "이상해꽃이");
assert.equal(josa("라이츄", "으로", "로", true), "라이츄로");
assert.equal(josa("리자몽", "으로", "로", true), "리자몽으로");
assert.equal(josa("에레브", "으로", "로", true), "에레브로");
assert.equal(josa("볼", "으로", "로", true), "볼로");
assert.equal(josa("Mew", "이", "가"), "Mew이(가)");
assert.equal(ko.notifEvolveBody("피카츄", "라이츄"), "피카츄가 라이츄로 진화했어요!");

// Stroke detector: three reversals of ≥ min distance inside 1.2 s.
const d = new PettingDetector(10);
const xs = [0, 12, 0, 12, 0];
const hits = xs.map((x, i) => d.feed(x, i * 0.1));
assert.deepEqual(hits, [false, false, false, false, true]);
const slow = new PettingDetector(10);
assert.deepEqual([0, 12, 0, 12, 0].map((x, i) => slow.feed(x, i * 1.3)), [false, false, false, false, false]);
const small = new PettingDetector(10);
assert.equal([0, 5, 0, 5, 0, 5, 0].some((x, i) => small.feed(x, i * 0.05)), false);
assert.equal(strokeDistance(48), 10);
assert.equal(strokeDistance(200), 30);

// Title fitting: whole when it fits, else the label is cut (at a word boundary when one is past half).
const width = (max: number) => (t: string) => Array.from(t).length <= max;
assert.equal(agentTitle("web", "Done!", "done", null, width(30)), "web · done");
assert.equal(agentTitle(null, "Your agent needs you", "waiting", "+2", width(30)), "Your agent needs you +2");
assert.equal(agentTitle("my-very-long-project-folder", "x", "waiting", "+3", width(24)), "my-very-lo… · waiting +3");
assert.equal(agentTitle("alpha beta gamma delta", "x", "done", null, width(20)), "alpha beta… · done");
assert.ok(Array.from(agentTitle("abcdefghijklmnopqrstuvwxyz", "x", "done", null, width(20))).length <= 20);
assert.equal(fittedBody("short", width(10)), "short");
assert.equal(fittedBody("hello wonderful world", width(10)), "hello won…");

// Bubble priority and content.
const agent = { done: { label: "api", message: null }, waiting: { label: null, message: "Approve?" }, other_waiting: 1, working: true };
const alert = { critical: true, window: "Claude weekly", utilization: 96, account: "me@x.com" };
assert.equal(pickBubble(alert, agent)?.kind, "limit");
assert.equal(pickBubble(null, agent)?.kind, "done");
assert.equal(pickBubble(null, { ...agent, done: null })?.kind, "waiting");
assert.equal(pickBubble(null, null), null);
const any = () => true;
assert.deepEqual(bubbleContent(pickBubble(alert, null)!, en, any, any), { title: "Limit imminent", body: "Claude weekly at 96% · me@x.com", critical: true });
assert.equal(bubbleContent(pickBubble(alert, null)!, en, any, (_, b) => b.length < 20).body, "Claude weekly at 96%");
assert.deepEqual(bubbleContent(pickBubble(null, agent)!, en, any, any), { title: "api · done", body: "Your agent finished its task.", critical: false });
assert.equal(bubbleContent(pickBubble(null, { ...agent, done: null })!, en, any, any).title, "Your agent needs you +1");

// Frame cap keeps the loop length.
const frames = Array.from({ length: 55 }, (_, i) => ({ image: i, delay: 0.05 }));
const capped = capFrameRate(frames, 0.4);
assert.deepEqual(capped.map((f) => f.image), [0, 8, 16, 24, 32, 40]);
assert.ok(Math.abs(capped.reduce((s, f) => s + f.delay, 0) - 2.75) < 1e-9);
assert.equal(capFrameRate(frames.slice(0, 1), 0.4).length, 1);
assert.deepEqual([frameFloor("smooth", false), frameFloor("balanced", false), frameFloor("smooth", true), frameFloor("powerSaver", false)], [0.1, 0.2, 0.4, 0.4]);

// Geometry helpers.
assert.deepEqual(panelSize(96, true), { w: 180, h: 168 });
assert.deepEqual(panelSize(240, true), { w: 240, h: 312 });
assert.deepEqual(panelSize(96, false), { w: 96, h: 96 });
assert.deepEqual([prayerEmojiSize(48), prayerEmojiSize(96), prayerEmojiSize(384)], [14, 24, 32]);
assert.equal(heartTop(20), -9);
