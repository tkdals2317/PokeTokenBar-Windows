// Home-usage live state: the day ledger and the selected provider tab. Owner: home-usage area.
// Other areas may import `useSelectedProviderId` read-only (LimitsSection shows only that provider).
import { useEffect, useState, useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { DailyLedger, UsageSnapshot } from "../../shared";

const EMPTY: DailyLedger = { tokens_by_day: {}, covered_since: null };

/** `usage-ledger.json` from Rust (`home_ledger`), updated on `home-ledger`. */
export function useLedger(): DailyLedger {
  const [ledger, setLedger] = useState(EMPTY);
  useEffect(() => {
    invoke<DailyLedger>("home_ledger").then(setLedger, () => {});
    const sub = listen<DailyLedger>("home-ledger", (e) => setLedger(e.payload));
    return () => {
      sub.then((f) => f());
    };
  }, []);
  return ledger;
}

// The provider tab survives reopening the popover but is never persisted (macOS `nav.providerID`).
let preferred: string | null = null;
const listeners = new Set<() => void>();
const subscribe = (cb: () => void) => {
  listeners.add(cb);
  return () => listeners.delete(cb);
};

export function selectProvider(id: string) {
  preferred = id;
  listeners.forEach((cb) => cb());
}

/** The preferred tab if it still has a snapshot, else the first snapshot (`UsageStore.snapshot(preferring:)`). */
export function useSelectedProviderId(usage: UsageSnapshot | null): string | null {
  const id = useSyncExternalStore(subscribe, () => preferred);
  const ps = usage?.providers ?? [];
  return ps.find((p) => p.provider_id === id)?.provider_id ?? ps[0]?.provider_id ?? null;
}
