// Pokédex entry page inside the Collection tab (shop-bag-dex.md §3.12, §5.3 detail; PokemonDetailView).
// Owner: shop-bag-dex area.
import { ReactNode, useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { typeColor } from "../../kit/colors";
import { Lang, useLang } from "../../kit/i18n";
import type { Companion, PokemonDetails, Settings } from "../../shared";
import { computeStats, displayScaleMaximum, resolveName } from "../../shared";
import { Sprite, useResourceNames } from "../companion/CompanionCard";
import { formLabel, formNameResource, forms, identifier, isDefaultForm, resolvedForm } from "../companion/forms";
import { STAT_ORDER } from "../companion/profile";
import { natureLabel } from "../companion/strings";
import { DexSpecies, dexSpecies, LogEntry, logEntries } from "./dex";
import { Glyph, rarityLabel, S } from "./ui";

/** Unown's 28 letters fill seven columns; named forms get wider cells. */
export const formPickerColumns = (n: number) => (n > 8 ? (n > 20 ? 7 : 6) : Math.min(n, 4));

function Name({ kind, name, suffix = "" }: { kind: string; name: string; suffix?: string }) {
  const lang = useLang();
  const r = useResourceNames(kind, name);
  return <>{(r?.names ? resolveName(r.names, lang) : null) ?? identifier(name)}{suffix}</>;
}

function TypePill({ type }: { type: string }) {
  return (
    <span className="sbd-type" style={{ background: typeColor(type) }}>
      <Name kind="type" name={type} />
    </span>
  );
}

const statLabel = (t: S, s: string) =>
  ({ hp: "HP", attack: t.statAttack, defense: t.statDefense, "special-attack": t.statSpAtk, "special-defense": t.statSpDef, speed: t.statSpeed })[s] ?? s;

function moveMethod(t: S, m: { method: string; level: number }) {
  switch (m.method) {
    case "level-up": return m.level > 0 ? `Lv. ${m.level}` : t.moveStart;
    case "machine": return "TM";
    case "egg": return t.moveEgg;
    case "light-ball-egg": return t.moveLightBall;
    case "form-change": return t.moveFormChange;
    case "tutor": return t.moveTutor;
    default: return m.method.replace(/-/g, " ");
  }
}

function StatRow({ t, name, value, iv, max }: { t: S; name: string; value: number; iv: number | null; max: number }) {
  return (
    <div className="sbd-stat">
      <span className="sbd-stat-label">{statLabel(t, name)}</span>
      <span className="sbd-stat-track"><span style={{ width: `${Math.min(100, (value / max) * 100)}%` }} /></span>
      <span className="sbd-stat-value num">{value}</span>
      {iv != null && <span className="sbd-stat-iv sbd-sec num">IV {iv}</span>}
    </div>
  );
}

const Pair = ({ label, value }: { label: string; value: ReactNode }) => (
  <div className="sbd-pair">
    <span className="sbd-sec">{label}</span>
    <span className="sbd-callout-semi">{value}</span>
  </div>
);

const genderLabel = (t: S, g: string | null | undefined) =>
  g === "male" ? t.genderMale : g === "female" ? t.genderFemale : g === "genderless" ? t.genderless : "—";

function useDetails(id: number) {
  const [state, setState] = useState<{ id: number; d: PokemonDetails | null; failed: boolean }>({ id, d: null, failed: false });
  const load = useCallback(() => {
    setState({ id, d: null, failed: false });
    invoke<PokemonDetails>("pokemon_details", { id }).then(
      (d) => setState({ id, d, failed: false }),
      () => setState({ id, d: null, failed: true }),
    );
  }, [id]);
  useEffect(load, [load]);
  return { ...(state.id === id ? state : { d: null, failed: false }), retry: load };
}

const formatDate = (iso: string, lang: Lang) => new Date(iso).toLocaleDateString(lang, { year: "numeric", month: "numeric", day: "numeric" });

export function DexDetail(props: {
  c: Companion;
  species: DexSpecies;
  initialForm: string | null;
  settings: Settings;
  save: (s: Settings) => void;
  t: S;
  onBack: () => void;
}) {
  const { c, species, settings, t } = props;
  const lang = useLang();
  const hasForms = forms(species.id).length > 0;
  const label = (id: number, f: string | null) => {
    const res = formNameResource(id, f);
    return formLabel(id, f, res != null ? resolveName(c.form_labels[res], lang) : null, t.normalForm);
  };
  const formSpecies = useMemo(
    () => (hasForms ? dexSpecies(c, lang, true, label).filter((s) => s.id === species.id) : []),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [c, lang, species.id, hasForms],
  );
  const isRep = (s: DexSpecies) =>
    settings.representative === s.id && (!s.form || resolvedForm(s.id, settings.representative_form) === s.form);
  const [selectedForm, setSelectedForm] = useState<string | null>(props.initialForm ? resolvedForm(species.id, props.initialForm) : null);
  const [selectedShiny, setSelectedShiny] = useState<boolean | null>(null);
  const [instance, setInstance] = useState("");

  const displayed = formSpecies.find((s) => s.form === selectedForm) ?? formSpecies.find(isRep) ?? formSpecies[0] ?? species;
  const all: LogEntry[] = useMemo(() => {
    const f = resolvedForm(species.id, displayed.form);
    const log = logEntries(c);
    const sorted = [...log.filter((e) => e.active), ...log.filter((e) => !e.active).sort((a, b) => (Date.parse(b.caught_at ?? "") || 0) - (Date.parse(a.caught_at ?? "") || 0))];
    return sorted.filter((e) => e.final_id === species.id && e.profile != null && resolvedForm(species.id, e.form) === f);
  }, [c, species.id, displayed.form]);
  const shiny =
    selectedShiny != null && (selectedShiny ? displayed.isShiny : displayed.hasNormal) ? selectedShiny : all[0]?.is_shiny ?? displayed.isShiny;
  const individuals = all.filter((e) => e.is_shiny === shiny);
  const individual = individuals.find((e) => e.id === instance) ?? individuals[0] ?? null;
  const { d, failed, retry } = useDetails(species.id);
  const rep = isRep(displayed);
  const setRep = () =>
    props.save({ ...settings, representative: rep ? null : displayed.id, representative_form: rep ? null : displayed.form });

  const allForms = forms(species.id);
  const cols = formPickerColumns(allForms.length);
  const individualLabel = (e: LogEntry, i: number) =>
    `#${i + 1} · ${e.is_shiny ? `✨ ${t.dexShinyLabel}` : t.dexNormalLabel} · Lv. ${e.profile?.level ?? 5} · ${
      e.caught_at ? formatDate(e.caught_at, lang) : e.active ? t.dexRaising : "—"
    }`;

  return (
    <div className="sbd-detail">
      <div className="sbd-detail-head">
        <button className="sbd-btn borderless sbd-back" onClick={props.onBack}>
          <Glyph name="chevronLeft" size={12} weight={2.4} />
          {t.back}
        </button>
        <span className="sbd-spacer" />
        <span className="sbd-callout sbd-sec">#{species.id}</span>
      </div>
      <div className="sbd-detail-scroll">
        {hasForms && (
          <div className="sbd-col6">
            <span className="sbd-callout-semi">{t.formsCollected(formSpecies.length, allForms.length)}</span>
            <div className="sbd-forms" style={{ gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))` }}>
              {allForms.map((f) => {
                const owned = formSpecies.find((s) => s.form === f);
                const l = label(species.id, f) ?? "";
                return (
                  <button
                    key={f}
                    className={`sbd-form${displayed.form === f ? " selected" : ""}${owned && isRep(owned) ? " rep" : ""}`}
                    disabled={!owned}
                    aria-label={`${species.name} [${l}]`}
                    title={owned ? (owned.isShiny ? `${l} · ${t.dexShinyLabel}` : l) : `${l} · ${t.formNotCollected}`}
                    onClick={() => {
                      setSelectedForm(f);
                      setInstance("");
                      setSelectedShiny(null);
                    }}
                  >
                    <span className="sbd-form-sprite">
                      <Sprite id={species.id} size={36} animated={false} shiny={owned?.isShiny === true} form={f} />
                      {owned?.isShiny && <span className="sbd-form-shiny">✨</span>}
                    </span>
                    <span className={cols > 4 ? "sbd-form-label one" : "sbd-form-label"}>{l}</span>
                  </button>
                );
              })}
            </div>
          </div>
        )}
        {displayed.hasNormal && displayed.isShiny && (
          <div className="kit-segmented" role="tablist" aria-label={t.dexAppearance}>
            {[false, true].map((v) => (
              <button key={String(v)} className={shiny === v ? "on" : ""} onClick={() => { setSelectedShiny(v); setInstance(""); }}>
                {v ? `✨ ${t.dexShinyLabel}` : t.dexNormalLabel}
              </button>
            ))}
          </div>
        )}
        <div className="sbd-identity">
          <Sprite id={displayed.id} size={104} shiny={shiny} form={displayed.form} />
          <div className="sbd-identity-text">
            <div className="sbd-identity-title">
              <span className="sbd-title3">{displayed.name}</span>
              <span className="sbd-spacer" />
              <span className="sbd-callout-semi sbd-sec">{rarityLabel(t, displayed.rarity)}</span>
            </div>
            {d && (
              <>
                <span className="sbd-types">{d.types.map((ty) => <TypePill key={ty} type={ty} />)}</span>
                {!isDefaultForm(displayed.id, displayed.form) && <span className="sbd-caption sbd-sec">{t.defaultFormDataNote}</span>}
              </>
            )}
            {shiny && <span className="sbd-callout">✨ {t.dexShinyLabel}</span>}
            {individual?.active && <span className="sbd-callout sbd-accent">{t.dexRaising}</span>}
            <RepButton t={t} rep={rep} onClick={setRep} />
          </div>
        </div>
        {individuals.length > 1 && (
          <label className="sbd-picker">
            <span>{t.individual}</span>
            <select value={individual?.id ?? ""} onChange={(e) => setInstance(e.target.value)}>
              {individuals.map((e, i) => <option key={e.id} value={e.id}>{individualLabel(e, i)}</option>)}
            </select>
          </label>
        )}
        {individuals.length === 0 && <span className="sbd-callout sbd-sec">{t.dexAppearancePreview}</span>}
        {d ? (
          <>
            {individual?.profile ? <IndividualCard t={t} e={individual} d={d} lang={lang} /> : <BaseStatsCard t={t} d={d} />}
            <SpeciesCard t={t} d={d} />
            <MovesCard t={t} d={d} />
          </>
        ) : failed ? (
          <div className="sbd-detail-state">
            <span className="sbd-sec">{t.detailsUnavailable}</span>
            <button className="sbd-btn" onClick={retry}>{t.retry}</button>
          </div>
        ) : (
          <div className="sbd-detail-state row sbd-sec">
            <span className="sbd-spinner" />
            {t.loadingDetails}
          </div>
        )}
      </div>
    </div>
  );
}

export function RepButton({ t, rep, onClick }: { t: S; rep: boolean; onClick: () => void }) {
  const title = rep ? t.representativeFollowCurrent : t.representativeSet;
  return (
    <button className="sbd-btn mini" title={title} aria-label={title} onClick={onClick}>
      <Glyph name={rep ? "follow" : "star"} size={11} />
    </button>
  );
}

function IndividualCard({ t, e, d, lang }: { t: S; e: LogEntry; d: PokemonDetails; lang: Lang }) {
  const p = e.profile!;
  const stats = computeStats(d, p, e.nature || null);
  const max = displayScaleMaximum(stats.map((s) => s.value));
  return (
    <div className="sbd-dcard">
      <span className="sbd-dtitle">{t.individual}</span>
      <div className="sbd-pairs">
        <Pair label={t.level} value={p.level} />
        <Pair label={t.gender} value={genderLabel(t, p.gender)} />
        <Pair label={t.nature} value={e.nature ? natureLabel(e.nature, lang) : "—"} />
      </div>
      {e.prayers && e.prayers.count > 0 && (
        <div className="sbd-col2">
          <span className="sbd-callout">{t.prayerBornWith(e.prayers.count)}</span>
          <span className="sbd-caption sbd-sec">{t.prayerReachedHeaven}</span>
        </div>
      )}
      <Pair
        label={t.ability}
        value={p.ability_name ? <Name kind="ability" name={p.ability_name} suffix={p.ability_is_hidden ? ` · ${t.hiddenAbility}` : ""} /> : "—"}
      />
      <span className="sbd-dtitle">{t.actualStats}</span>
      {stats.map((s) => <StatRow key={s.name} t={t} name={s.name} value={s.value} iv={s.iv} max={max} />)}
      <span className="sbd-dtitle">{t.activeMoves}</span>
      {p.moves.length === 0 ? (
        <span className="sbd-callout sbd-sec">{t.noLevelMoves}</span>
      ) : (
        p.moves.map((m) => (
          <div key={m.name} className="sbd-move sbd-callout">
            <Name kind="move" name={m.name} />
            <span className="sbd-sec">Lv. {m.learned_at_level}</span>
          </div>
        ))
      )}
    </div>
  );
}

function BaseStatsCard({ t, d }: { t: S; d: PokemonDetails }) {
  return (
    <div className="sbd-dcard">
      <span className="sbd-dtitle">{t.baseStats}</span>
      {STAT_ORDER.filter((s) => d.base_stats[s] != null).map((s) => (
        <StatRow key={s} t={t} name={s} value={d.base_stats[s]} iv={null} max={300} />
      ))}
    </div>
  );
}

function SpeciesCard({ t, d }: { t: S; d: PokemonDetails }) {
  const total = Object.values(d.base_stats).reduce((a, b) => a + b, 0);
  return (
    <div className="sbd-dcard">
      <span className="sbd-dtitle">{t.speciesData}</span>
      <div className="sbd-pairs wide">
        <Pair label={t.height} value={`${(d.height / 10).toFixed(1)} m`} />
        <Pair label={t.weight} value={`${(d.weight / 10).toFixed(1)} kg`} />
        <Pair label={t.baseStatTotal} value={total} />
      </div>
      <span className="sbd-dtitle">{t.possibleAbilities}</span>
      <span className="sbd-callout sbd-sec">
        {d.abilities.map((a, i) => (
          <span key={a.name}>
            {i > 0 && ", "}
            <Name kind="ability" name={a.name} suffix={a.is_hidden ? ` (${t.hidden})` : ""} />
          </span>
        ))}
      </span>
    </div>
  );
}

function MovesCard({ t, d }: { t: S; d: PokemonDetails }) {
  return (
    <div className="sbd-dcard">
      <span className="sbd-dtitle">{t.completeMoveList(d.moves.length)}</span>
      {d.moves.map((m) => (
        <div key={m.name} className="sbd-move sbd-callout divided">
          <Name kind="move" name={m.name} />
          <span className="sbd-sec sbd-right">{[...new Set(m.learn_methods.map((x) => moveMethod(t, x)))].join(" · ")}</span>
        </div>
      ))}
    </div>
  );
}
