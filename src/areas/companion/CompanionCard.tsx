// Companion header on Home (companion.md §5). Owner: companion area. Home-usage renders it as a slot.
import { compact } from "../../kit/format";
import { natureName } from "../../i18n";
import { EGG, TabProps, nameOf, sprite, useI18n } from "../../shared";

function Progress({ value, max, label }: { value: number; max: number; label: string }) {
  const pct = Math.min(100, (value / max) * 100);
  return (
    <div className="progress">
      <div className="bar"><div style={{ width: `${pct}%` }} /></div>
      <p className="muted">{label} · {compact(value)} / {compact(max)}</p>
    </div>
  );
}

/** Home slot: receives the full TabProps so its inputs can change without touching HomeTab. */
export default function CompanionCard({ companion: c }: TabProps) {
  const { t, lang } = useI18n();
  if (!c) return null;
  const mon = c.active;
  if (!mon) {
    return (
      <section className="card center">
        <img className="sprite" src={EGG} alt="" />
        <h2>{t.egg}</h2>
        <Progress value={c.egg_usage} max={c.egg_threshold} label={t.toHatch} />
      </section>
    );
  }
  const id = mon.path_ids[mon.stage_index];
  const isFinal = mon.stage_index === mon.planned_ids.length - 1;
  return (
    <section className="card center">
      <img className="sprite" src={sprite(id, mon.is_shiny)} alt="" />
      <h2>
        {mon.is_shiny && "✨ "}
        {nameOf(mon.line.names, id, lang)}
      </h2>
      <p className="muted">
        <span className={`badge ${mon.line.rarity}`}>{t.rarity[mon.line.rarity]}</span>{" "}
        {natureName(mon.nature, lang)} {t.natureSuffix}
      </p>
      <div className="evo">
        {mon.planned_ids.map((pid, i) =>
          i <= mon.stage_index ? (
            <img key={pid} src={sprite(pid, mon.is_shiny)} alt={nameOf(mon.line.names, pid, lang)} />
          ) : (
            <span key={pid} className="mystery">?</span>
          ),
        )}
      </div>
      <Progress value={mon.used_at_stage} max={c.threshold ?? 1} label={isFinal ? t.toGraduate : t.toEvolve} />
    </section>
  );
}
