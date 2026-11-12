// Provider registry (mirrors `providers::PROVIDERS`). Owner: providers.
// Registration order = tab order, Settings toggle order and activity palette slot (index % 16).

export const PROVIDERS: readonly (readonly [id: string, name: string])[] = [
  ["claude_code", "Claude Code"],
  ["codex", "Codex"],
  ["gemini", "Gemini"],
  ["antigravity", "Antigravity"],
  ["opencode", "OpenCode"],
  ["hermes", "Hermes Agent"],
  ["cursor", "Cursor"],
  ["grok", "Grok"],
  ["copilot", "Copilot"],
  ["kiro", "Kiro"],
  ["pi", "Pi"],
  ["omp", "omp"],
  ["omo", "omo"],
  ["aside", "Aside"],
  ["kimi_code", "Kimi Code"],
  ["devin", "Devin"],
];

/** The opt-out list after switching `id` on or off: sorted, no duplicates. */
export function withProviderEnabled(disabled: string[], id: string, enabled: boolean): string[] {
  const set = new Set(disabled);
  if (enabled) set.delete(id);
  else set.add(id);
  return [...set].sort();
}

/** `custom_scan_roots` with `id` set to `raw`; a blank value removes the key. */
export function withScanRoots(map: Record<string, string>, id: string, raw: string): Record<string, string> {
  const next = { ...map };
  if (raw.trim()) next[id] = raw;
  else delete next[id];
  return next;
}
