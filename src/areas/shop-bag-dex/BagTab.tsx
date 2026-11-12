// Bag tab (shop-bag-dex.md §5.2, BagView.swift). Owner: shop-bag-dex area.
// Inline confirm; using an item always switches to Home, where the card plays the feedback.
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./shop-bag-dex.css";
import { compact } from "../../kit/format";
import { useLang, useStrings } from "../../kit/i18n";
import type { Companion, TabProps } from "../../shared";
import { Sprite } from "../companion/CompanionCard";
import { Economy, ItemKey, planCandy, useEconomy } from "./economy";
import { sbdStrings } from "./strings";
import { activeName, Glyph, ItemIcon, itemDescription, itemName, S, Stepper } from "./ui";

function ItemCard({ item, count, e, c, t, goTo }: { item: ItemKey; count: number; e: Economy; c: Companion; t: S; goTo: TabProps["goTo"] }) {
  const lang = useLang();
  const passive = item === "shinyCharm";
  const [confirming, setConfirming] = useState(false);
  const [candyCount, setCandyCount] = useState(1);
  const maxCandy = Math.max(1, e.max_candy_use);
  const q = Math.min(candyCount, maxCandy);
  useEffect(() => setCandyCount((v) => Math.min(v, maxCandy)), [maxCandy]);
  const canUse = item === "rareCandy" ? e.max_candy_use > 0 : item === "mint" ? e.can_use_mint : false;
  const preview = item === "rareCandy" ? planCandy(q, e, c.active?.used_at_stage ?? 0) : null;
  const name = itemName(t, item);

  const useNow = async () => {
    setConfirming(false);
    await invoke("use_item", { item, count: item === "rareCandy" ? q : 1 });
    goTo("home");
  };

  return (
    <div className="sbd-card sbd-item">
      <div className="sbd-item-top">
        <ItemIcon item={item} />
        <div className="sbd-item-text">
          <div className="sbd-item-title">
            <span className="sbd-callout-semi">{name}</span>
            {!passive && <span className="sbd-caption sbd-bold sbd-sec num">×{count}</span>}
            <span className="sbd-spacer" />
            {item === "rareCandy" && canUse && <Stepper value={q} max={maxCandy} onChange={setCandyCount} label={name} />}
          </div>
          <span className="sbd-caption sbd-sec">{itemDescription(t, item, e.candy_xp)}</span>
        </div>
      </div>
      {preview?.graduates && <span className="sbd-caption sbd-sec">{t.candyGraduates}</span>}
      {preview && preview.discardedXP > 0 ? (
        <span className="sbd-caption sbd-orange">{t.candyDiscardedXP(compact(preview.discardedXP))}</span>
      ) : (
        preview?.evolves && !preview.graduates && <span className="sbd-caption sbd-sec">{t.candyCarryoverXP(compact(preview.carryoverXP))}</span>
      )}
      {passive ? (
        <div className="sbd-controls sbd-green">
          <Glyph name="sealCheck" size={11} />
          <span className="sbd-caption sbd-semi">{t.shinyCharmEffectHint}</span>
        </div>
      ) : !canUse ? (
        <span className="sbd-caption sbd-ter">{c.active ? t.useNeedsPokemon : t.useAfterHatch}</span>
      ) : confirming ? (
        <div className="sbd-controls">
          <span className="sbd-caption sbd-sec sbd-line1">{t.useOnCurrent(activeName(c, lang, t.normalForm))}</span>
          <span className="sbd-spacer" />
          <button className="sbd-btn prominent" onClick={useNow}>{item === "rareCandy" ? `${t.use} ×${q}` : t.use}</button>
          <button className="sbd-btn borderless" onClick={() => setConfirming(false)}>{t.cancel}</button>
        </div>
      ) : (
        <div className="sbd-controls">
          <span className="sbd-caption sbd-ter num">{item === "rareCandy" ? `+${compact(q * e.candy_xp)} XP` : t.mintEffectHint}</span>
          <span className="sbd-spacer" />
          <button className="sbd-btn" onClick={() => setConfirming(true)}>{t.useItem}</button>
        </div>
      )}
    </div>
  );
}

export default function BagTab({ companion, goTo }: TabProps) {
  const t = useStrings(sbdStrings);
  const e = useEconomy();
  if (!e || !companion) return null;
  if (e.owned.length === 0) {
    return (
      <div className="sbd-empty">
        <Sprite id={143} size={96} />
        <span className="sbd-callout-semi">{t.bagEmptyTitle}</span>
        <button className="sbd-btn" onClick={() => goTo("shop")}>{t.shop}</button>
      </div>
    );
  }
  return (
    <div className="sbd-list sbd-gap8">
      {e.owned.map((o) => (
        <ItemCard key={o.item} item={o.item} count={o.count} e={e} c={companion} t={t} goTo={goTo} />
      ))}
    </div>
  );
}
