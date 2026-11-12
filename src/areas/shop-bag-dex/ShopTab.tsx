// Shop tab (shop-bag-dex.md §5.1, ShopView.swift). Owner: shop-bag-dex area.
// Inline confirms (the button morphs), never a modal.
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./shop-bag-dex.css";
import { Badge } from "../../kit/components";
import { rarityColor } from "../../kit/colors";
import { compact } from "../../kit/format";
import { useLang, useStrings } from "../../kit/i18n";
import type { Companion, TabProps } from "../../shared";
import { Sprite } from "../companion/CompanionCard";
import { Economy, ShopEntry, useEconomy } from "./economy";
import { sbdStrings } from "./strings";
import { activeName, eggName, Glyph, ItemIcon, itemDescription, itemName, rarityLabel, S, Stepper } from "./ui";

function Wallet({ e, t }: { e: Economy; t: S }) {
  return (
    <div className="sbd-card sbd-wallet">
      <span className="sbd-caption sbd-sec">{t.spendableTokens}</span>
      <span className="sbd-wallet-amount num">{compact(e.available)}</span>
      <span className="sbd-caption sbd-ter">{t.shopHint}</span>
    </div>
  );
}

function ItemCard({ entry, e, t }: { entry: ShopEntry; e: Economy; t: S }) {
  const item = entry.item!;
  const passive = item === "shinyCharm";
  const owned = e.owned.find((o) => o.item === item)?.count ?? 0;
  const [confirming, setConfirming] = useState(false);
  const [quantity, setQuantity] = useState(1);
  const maxQuantity = Math.max(1, entry.max_buy);
  const q = Math.min(quantity, maxQuantity);
  useEffect(() => setQuantity((v) => Math.min(v, maxQuantity)), [maxQuantity]);
  const name = itemName(t, item);
  const quantityName = q > 1 ? `${name} ×${q}` : name;

  const buyNow = async () => {
    setConfirming(false);
    if (await invoke<boolean>("buy_item", { item, count: q })) setQuantity(1);
  };

  return (
    <div className="sbd-card sbd-item">
      <div className="sbd-item-top">
        <ItemIcon item={item} />
        <div className="sbd-item-text">
          <div className="sbd-item-title">
            <span className="sbd-callout-semi">{name}</span>
            {owned > 0 && !passive && <span className="sbd-caption sbd-bold sbd-sec num">{t.ownedCount(owned)}</span>}
            <span className="sbd-spacer" />
            {!passive && entry.max_buy > 1 && <Stepper value={q} max={maxQuantity} onChange={setQuantity} label={name} />}
          </div>
          <span className="sbd-caption sbd-sec">{itemDescription(t, item, e.candy_xp)}</span>
        </div>
      </div>
      {passive && owned > 0 ? (
        <div className="sbd-controls sbd-green">
          <Glyph name="sealCheck" size={11} />
          <span className="sbd-caption sbd-semi">{t.ownedAlready}</span>
        </div>
      ) : confirming ? (
        <div className="sbd-controls">
          <span className="sbd-caption sbd-sec sbd-line1">{t.buyConfirm(quantityName)}</span>
          <span className="sbd-spacer" />
          <button className="sbd-btn prominent" onClick={buyNow}>{q > 1 ? `${t.buy} ×${q}` : t.buy}</button>
          <button className="sbd-btn borderless" onClick={() => setConfirming(false)}>{t.cancel}</button>
        </div>
      ) : (
        <div className="sbd-controls">
          <span className="sbd-caption sbd-ter num">{`${t.priceLabel} ${compact(entry.price * q)}`}</span>
          <span className="sbd-spacer" />
          {entry.can_buy ? (
            <button className="sbd-btn" onClick={() => setConfirming(true)}>{t.buy}</button>
          ) : (
            <span className="sbd-caption sbd-ter">{t.notEnoughTokens}</span>
          )}
        </div>
      )}
    </div>
  );
}

function EggCard({ entry, c, t, goTo }: { entry: ShopEntry; c: Companion; t: S; goTo: TabProps["goTo"] }) {
  const lang = useLang();
  const [stage, setStage] = useState<"idle" | "confirm" | "precious">("idle");
  const hasActive = c.active != null;
  const tier = entry.tier;
  const name = eggName(t, tier);
  const legendary = c.active?.line.rarity === "legendary";
  const highValue = c.displays_shiny || legendary;

  const commit = async () => {
    setStage("idle");
    if (await invoke<boolean>("buy_egg", { tier })) goTo("home");
  };

  return (
    <div className="sbd-card sbd-item">
      <div className="sbd-item-top">
        <span className="sbd-item-icon" style={{ width: 30, height: 30 }}>
          <Sprite id={null} size={26} />
        </span>
        <div className="sbd-item-text">
          <div className="sbd-item-title">
            <span className="sbd-callout-semi">{name}</span>
            {tier && <Badge text={rarityLabel(t, tier).toUpperCase()} tint={rarityColor(tier)} />}
          </div>
          <span className="sbd-caption sbd-sec">{tier ? t.eggDescriptionTier(rarityLabel(t, tier)) : t.eggDescriptionPlain}</span>
          {hasActive && <span className="sbd-caption sbd-ter">{t.eggReleaseNote}</span>}
        </div>
      </div>
      {stage === "idle" ? (
        <div className="sbd-col4">
          <div className="sbd-controls">
            <span className="sbd-caption sbd-ter num">{`${t.priceLabel} ${compact(entry.price)}`}</span>
            <span className="sbd-spacer" />
            {!hasActive ? (
              <button className="sbd-btn" disabled>{t.buy}</button>
            ) : entry.can_buy ? (
              <button className="sbd-btn" onClick={() => setStage("confirm")}>{t.buy}</button>
            ) : (
              <span className="sbd-caption sbd-ter">{t.notEnoughTokens}</span>
            )}
          </div>
          {!hasActive && <span className="sbd-caption sbd-ter">{t.eggShopLockedHint}</span>}
        </div>
      ) : stage === "confirm" ? (
        <div className="sbd-controls">
          <span className="sbd-caption sbd-sec sbd-line2">{t.eggConfirm(activeName(c, lang, t.normalForm), name)}</span>
          <span className="sbd-spacer" />
          <button className="sbd-btn prominent" onClick={() => (highValue ? setStage("precious") : commit())}>{t.buy}</button>
          <button className="sbd-btn borderless" onClick={() => setStage("idle")}>{t.cancel}</button>
        </div>
      ) : (
        <div className="sbd-controls">
          <span className="sbd-caption sbd-semi sbd-orange sbd-line2">{legendary ? t.freshEggLegendaryWarning : t.freshEggShinyWarning}</span>
          <span className="sbd-spacer" />
          <button className="sbd-btn prominent orange" onClick={commit}>
            {c.displays_shiny && !legendary ? t.freshEggDiscardShiny : t.freshEggDiscardValuable}
          </button>
          <button className="sbd-btn borderless" onClick={() => setStage("idle")}>{t.cancel}</button>
        </div>
      )}
    </div>
  );
}

export default function ShopTab({ companion, goTo }: TabProps) {
  const t = useStrings(sbdStrings);
  const e = useEconomy();
  if (!e || !companion) return null;
  return (
    <div className="sbd-list">
      <Wallet e={e} t={t} />
      {e.shop.map((entry) =>
        entry.kind === "item" ? (
          <ItemCard key={entry.item} entry={entry} e={e} t={t} />
        ) : (
          <EggCard key={entry.tier ?? "plain"} entry={entry} c={companion} t={t} goTo={goTo} />
        ),
      )}
    </div>
  );
}
