// Limits data hooks (own command + event, ARCHITECTURE.md §3 Frontend 6). Owner: limits area.
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ProviderLimits } from "../../shared";

export type ProviderStatus = {
  provider: string;
  indicator: "operational" | "minor" | "major" | "critical" | "maintenance" | "unknown";
  description: string;
};
export type Org = { id: string; name: string };
export type SessionKeyView = {
  account: string;
  folder: string;
  title: string;
  configured: boolean;
  expired: boolean;
  organizations: Org[];
  organization_id: string | null;
};
export type LimitsExtras = {
  statuses: ProviderStatus[];
  session_keys: SessionKeyView[];
  claude_pending: boolean;
  claude_accounts: number;
};
/** Payload of `limits-alert` and `limit_bubble` (the floating pet shows it for 6 s). */
export type LimitAlert = { key: string; title: string; body: string; critical: boolean; utilization: number };

const EMPTY: LimitsExtras = { statuses: [], session_keys: [], claude_pending: false, claude_accounts: 0 };

export function useLimitsExtras(): LimitsExtras {
  const [extras, setExtras] = useState(EMPTY);
  useEffect(() => {
    invoke<LimitsExtras>("limits_extras").then(setExtras);
    const sub = listen<LimitsExtras>("limits-extras", (e) => setExtras(e.payload));
    return () => {
      sub.then((f) => f());
    };
  }, []);
  return extras;
}

/** The `limits` rows outside the popover body (Settings has no TabProps.limits). */
export function useLimits(): ProviderLimits[] {
  const [limits, setLimits] = useState<ProviderLimits[]>([]);
  useEffect(() => {
    invoke<ProviderLimits[]>("limits").then(setLimits);
    const sub = listen<ProviderLimits[]>("limits", (e) => setLimits(e.payload));
    return () => {
      sub.then((f) => f());
    };
  }, []);
  return limits;
}

/** Re-render every `ms` so relative times tick like SwiftUI `Text(date, style: .relative)`. */
export function useNow(ms = 1000) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), ms);
    return () => window.clearInterval(id);
  }, [ms]);
  return now;
}

/** "Refresh" / "Retry": re-reads every token, ignores the 429 backoff. Resolves to an error code. */
export function useManualRefresh() {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const run = async () => {
    if (busy) return;
    setBusy(true);
    try {
      setError(await invoke<string | null>("limits_refresh"));
    } finally {
      setBusy(false);
    }
  };
  return { busy, error, run, clear: () => setError(null) };
}

let focusSessionKey = false;

/** "Settings" / "Register Session Key": opens Settings and scrolls to the session key field. */
export function openSessionKeySettings() {
  focusSessionKey = true;
  invoke("show_popover", { opened: { tab: null, settings: true } });
}

export function takeSessionKeyFocus() {
  const v = focusSessionKey;
  focusSessionKey = false;
  return v;
}

/** The Claude account tab, kept for the app's lifetime like macOS `nav.claudeAccountID`. */
let claudeAccountId: string | null = null;
export const getClaudeAccount = () => claudeAccountId;
export const setClaudeAccount = (id: string) => {
  claudeAccountId = id;
};
