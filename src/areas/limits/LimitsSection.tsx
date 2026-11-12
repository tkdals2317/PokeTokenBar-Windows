// Official limits on Home (limits.md §5). Owner: limits area. Home-usage renders it as a slot.
import { LegacyDict } from "../../i18n";
import { ProviderLimits, Settings, useI18n } from "../../shared";

const PROVIDER: Record<string, string> = { claude: "Claude Code", codex: "Codex" };

function resetIn(iso: string | null, t: LegacyDict) {
  if (!iso) return "";
  const mins = Math.max(0, Math.round((Date.parse(iso) - Date.now()) / 60000));
  const d = Math.floor(mins / 1440), h = Math.floor((mins % 1440) / 60), m = mins % 60;
  const text = d > 0 ? `${d}${t.days} ${h}${t.hours}` : h > 0 ? `${h}${t.hours} ${m}${t.minutes}` : `${m}${t.minutes}`;
  return t.resetIn(text);
}

function windowName(key: string, t: LegacyDict) {
  if (t.windows[key]) return t.windows[key];
  const [, n, unit] = key.match(/^(\d+)([dhm])$/) ?? [];
  return n ? `${n}${unit === "d" ? t.days : unit === "h" ? t.hours : t.minutes}` : key;
}

export default function LimitsSection({ all, settings }: { all: ProviderLimits[]; settings: Settings }) {
  const { t } = useI18n();
  const mode = settings.limit_display;
  if (all.length === 0) return null;
  return (
    <section className="card">
      <strong>{t.officialLimits}</strong>
      {all.map((p) => (
        <div key={p.provider} className="limit-provider">
          <p className="muted">{PROVIDER[p.provider] ?? p.provider}</p>
          {p.error && <p className="muted">{t.limitErrors[p.error] ?? p.error}</p>}
          {p.windows.map((w) => {
            const shown = mode === "remaining" ? 100 - w.used_percent : w.used_percent;
            const tone = w.used_percent >= 95 ? "danger" : w.used_percent >= 80 ? "warn" : "";
            return (
              <div key={w.key} className="limit">
                <div className="row">
                  <span>{windowName(w.key, t)}</span>
                  <span>{mode === "remaining" ? t.remaining : t.used} {Math.round(shown)}%</span>
                </div>
                <div className="bar">
                  <div className={tone} style={{ width: `${Math.min(100, Math.max(0, shown))}%` }} />
                </div>
                <p className="muted">{resetIn(w.resets_at, t)}</p>
              </div>
            );
          })}
        </div>
      ))}
    </section>
  );
}
