import assert from "node:assert/strict";
import { PROVIDERS, withProviderEnabled, withScanRoots } from "./registry";

assert.equal(PROVIDERS.length, 16);
assert.deepEqual([PROVIDERS[0][0], PROVIDERS[15][0]], ["claude_code", "devin"]);

assert.deepEqual(withProviderEnabled(["grok"], "codex", false), ["codex", "grok"]);
assert.deepEqual(withProviderEnabled(["codex", "grok"], "codex", true), ["grok"]);
assert.deepEqual(withProviderEnabled(["codex"], "codex", false), ["codex"]);

assert.deepEqual(withScanRoots({ pi: "~/a" }, "gemini", "~/g"), { pi: "~/a", gemini: "~/g" });
assert.deepEqual(withScanRoots({ pi: "~/a" }, "pi", "  \n"), {});
