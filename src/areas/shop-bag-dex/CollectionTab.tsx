import { useMemo, useState } from "react";
// Collection tab (Pokédex + Catch log). Owner: shop-bag-dex area.
import { DexEntry, OwnedSpecies, Rarity, Settings, TabProps, nameOf, ownedSpecies, sprite, useI18n, Companion } from "../../shared";
import { natureName } from "../../i18n";

const PAGE_SIZE = 16; // 4 × 4, like the original
const RARITIES: Rarity[] = ["common", "uncommon", "rare", "legendary"];

/** "25", "#25" or any localized name fragment. */
function matches(query: string, id: number, names: string[]) {
  const q = query.trim().toLowerCase().replace(/^#/, "");
  if (!q) return true;
  if (/^\d+$/.test(q)) return String(id).startsWith(q);
  return names.some((n) => n.toLowerCase().includes(q));
}

const allNames = (names: Record<string, string> | undefined) => Object.values(names ?? {});

function Filters(props: {
  query: string;
  setQuery: (q: string) => void;
  rarity: Rarity | "";
  setRarity: (r: Rarity | "") => void;
  shinyOnly?: boolean;
  setShinyOnly?: (b: boolean) => void;
  sort: string;
  setSort: (s: string) => void;
  sorts: [string, string][];
}) {
  const { t } = useI18n();
  return (
    <div className="filters">
      <input value={props.query} placeholder={t.search} onChange={(e) => props.setQuery(e.target.value)} />
      <div className="row">
        <select value={props.rarity} onChange={(e) => props.setRarity(e.target.value as Rarity | "")}>
          <option value="">{t.allRarities}</option>
          {RARITIES.map((r) => (
            <option key={r} value={r}>{t.rarity[r]}</option>
          ))}
        </select>
        <select value={props.sort} onChange={(e) => props.setSort(e.target.value)}>
          {props.sorts.map(([v, label]) => (
            <option key={v} value={v}>{label}</option>
          ))}
        </select>
        {props.setShinyOnly && (
          <label className="check">
            <input type="checkbox" checked={props.shinyOnly} onChange={(e) => props.setShinyOnly!(e.target.checked)} />
            ✨ {t.shinyOnly}
          </label>
        )}
      </div>
    </div>
  );
}

function SpeciesDetail(props: {
  s: OwnedSpecies;
  settings: Settings;
  saveSettings: (s: Settings) => void;
  onClose: () => void;
}) {
  const { t, lang } = useI18n();
  const { s, settings } = props;
  const [shiny, setShiny] = useState(s.shiny);
  const isRep = settings.representative === s.id;
  return (
    <div className="modal" onClick={props.onClose}>
      <div className="card center" onClick={(e) => e.stopPropagation()}>
        <img className="sprite big" src={sprite(s.id, shiny)} alt="" />
        <h2>
          {shiny && "✨ "}
          {nameOf(s.names, s.id, lang)}
        </h2>
        <p className="muted">
          #{String(s.id).padStart(3, "0")} · <span className={`badge ${s.rarity}`}>{t.rarity[s.rarity]}</span>
          {s.count > 0 && ` · ${t.caughtCount(s.count)}`}
          {s.growing && ` · ${t.growing}`}
        </p>
        {s.shiny && (
          <button className="link" onClick={() => setShiny(!shiny)}>
            {shiny ? t.normalColor : t.shinyColor}
          </button>
        )}
        <div className="actions">
          {isRep ? (
            <button onClick={() => props.saveSettings({ ...settings, representative: null })}>
              ★ {t.isRepresentative} · {t.followCurrent}
            </button>
          ) : (
            <button onClick={() => props.saveSettings({ ...settings, representative: s.id })}>
              {t.setRepresentative}
            </button>
          )}
          <button onClick={props.onClose}>{t.close}</button>
        </div>
      </div>
    </div>
  );
}

function SpeciesGrid({ c, settings, saveSettings }: { c: Companion; settings: Settings; saveSettings: (s: Settings) => void }) {
  const { t, lang } = useI18n();
  const [query, setQuery] = useState("");
  const [rarity, setRarity] = useState<Rarity | "">("");
  const [shinyOnly, setShinyOnly] = useState(false);
  const [sort, setSort] = useState("number");
  const [page, setPage] = useState(0);
  const [open, setOpen] = useState<OwnedSpecies | null>(null);

  const species = useMemo(() => {
    const list = [...ownedSpecies(c).values()].filter(
      (s) =>
        matches(query, s.id, allNames(s.names[s.id])) &&
        (!rarity || s.rarity === rarity) &&
        (!shinyOnly || s.shiny),
    );
    if (sort === "name") list.sort((a, b) => nameOf(a.names, a.id, lang).localeCompare(nameOf(b.names, b.id, lang), lang));
    else list.sort((a, b) => (sort === "number-desc" ? b.id - a.id : a.id - b.id));
    return list;
  }, [c, query, rarity, shinyOnly, sort, lang]);

  const pages = Math.max(1, Math.ceil(species.length / PAGE_SIZE));
  const current = Math.min(page, pages - 1);
  const reset = <T,>(set: (v: T) => void) => (v: T) => {
    set(v);
    setPage(0);
  };

  return (
    <>
      <Filters
        query={query}
        setQuery={reset(setQuery)}
        rarity={rarity}
        setRarity={reset(setRarity)}
        shinyOnly={shinyOnly}
        setShinyOnly={reset(setShinyOnly)}
        sort={sort}
        setSort={setSort}
        sorts={[["number", t.sortNumber], ["number-desc", t.sortNumberDesc], ["name", t.sortName]]}
      />
      {species.length === 0 ? (
        <p className="muted center">{t.noMatch}</p>
      ) : (
        <div className="dex-grid">
          {species.slice(current * PAGE_SIZE, (current + 1) * PAGE_SIZE).map((s) => (
            <button key={s.id} className="dex-cell" onClick={() => setOpen(s)}>
              <img src={sprite(s.id, false)} alt="" />
              <span className="dex-no">
                #{String(s.id).padStart(3, "0")} {s.shiny && "✨"}
                {settings.representative === s.id && "★"}
              </span>
              <span className="dex-name">{nameOf(s.names, s.id, lang)}</span>
            </button>
          ))}
        </div>
      )}
      {pages > 1 && (
        <div className="pager">
          <button disabled={current === 0} onClick={() => setPage(current - 1)}>‹</button>
          <span className="muted">{t.page(current + 1, pages)}</span>
          <button disabled={current === pages - 1} onClick={() => setPage(current + 1)}>›</button>
        </div>
      )}
      {open && (
        <SpeciesDetail s={open} settings={settings} saveSettings={saveSettings} onClose={() => setOpen(null)} />
      )}
    </>
  );
}

function CatchLog({ dex }: { dex: DexEntry[] }) {
  const { t, lang } = useI18n();
  const [query, setQuery] = useState("");
  const [rarity, setRarity] = useState<Rarity | "">("");
  const [sort, setSort] = useState("newest");

  const entries = useMemo(() => {
    const list = dex.filter(
      (d) =>
        matches(query, d.final_id, d.chain.flatMap((id) => allNames(d.names[id]))) &&
        (!rarity || d.rarity === rarity),
    );
    list.sort((a, b) => a.caught_at.localeCompare(b.caught_at) * (sort === "newest" ? -1 : 1));
    return list;
  }, [dex, query, rarity, sort]);

  return (
    <>
      <Filters
        query={query}
        setQuery={setQuery}
        rarity={rarity}
        setRarity={setRarity}
        sort={sort}
        setSort={setSort}
        sorts={[["newest", t.sortNewest], ["oldest", t.sortOldest]]}
      />
      {entries.length === 0 && <p className="muted center">{t.noMatch}</p>}
      {entries.map((d, i) => (
        <div key={`${d.caught_at}-${i}`} className="log-row">
          <div className="evo">
            {d.chain.map((id) => (
              <img key={id} src={sprite(id, d.is_shiny)} alt={nameOf(d.names, id, lang)} />
            ))}
          </div>
          <div>
            <strong>
              {d.is_shiny && "✨ "}
              {nameOf(d.names, d.final_id, lang)}
            </strong>
            <p className="muted">
              <span className={`badge ${d.rarity}`}>{t.rarity[d.rarity]}</span> {natureName(d.nature, lang)} ·{" "}
              {new Date(d.caught_at).toLocaleDateString(lang)}
            </p>
          </div>
        </div>
      ))}
    </>
  );
}

export default function CollectionTab({ companion, settings, save }: TabProps) {
  return companion && <Dex c={companion} settings={settings} saveSettings={save} />;
}

function Dex(props: { c: Companion; settings: Settings; saveSettings: (s: Settings) => void }) {
  const { t } = useI18n();
  const [tab, setTab] = useState<"dex" | "log">("dex");
  const empty = props.c.dex.length === 0 && !props.c.active;
  return (
    <section className="card">
      <div className="segmented">
        <button className={tab === "dex" ? "on" : ""} onClick={() => setTab("dex")}>{t.dexTab}</button>
        <button className={tab === "log" ? "on" : ""} onClick={() => setTab("log")}>
          {t.catchLogTab} {props.c.dex.length}
        </button>
      </div>
      {empty ? (
        <p className="muted center">{t.emptyDex}</p>
      ) : tab === "dex" ? (
        <SpeciesGrid c={props.c} settings={props.settings} saveSettings={props.saveSettings} />
      ) : (
        <CatchLog dex={props.c.dex} />
      )}
    </section>
  );
}
