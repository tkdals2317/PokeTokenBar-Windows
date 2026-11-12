// Home tab body (home-usage.md §5.2). Owner: home-usage area.
// Slots owned by other areas: <CompanionCard> (companion), <LimitsSection> (limits).
import CompanionCard from "../companion/CompanionCard";
import LimitsSection from "../limits/LimitsSection";
import { compact } from "../../kit/format";
import { TabProps, todayKey, todayTotal, useI18n } from "../../shared";

export default function HomeTab({ usage, companion, limits, settings }: TabProps) {
  const { t } = useI18n();
  const day = todayKey();
  const today = (n: number | undefined) => compact(n ?? 0);
  return (
    <div className="home">
      {companion && <CompanionCard c={companion} />}
      {usage && (
        <section className="card">
          <div className="row">
            <strong>{t.today}</strong>
            <strong>{compact(todayTotal(usage, day))}</strong>
          </div>
          {usage.providers.map((p) => (
            <div key={p.provider_id} className="row muted">
              <span>{p.display_name}</span>
              <span>{today(p.today?.date === day ? p.today.total : 0)}</span>
            </div>
          ))}
        </section>
      )}
      <LimitsSection all={limits} settings={settings} />
    </div>
  );
}
