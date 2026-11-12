// Economy view from Rust (src-tauri/src/economy) and the Rare Candy preview. Owner: shop-bag-dex area.
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Rarity } from "../../shared";

export type ItemKey = "rareCandy" | "mint" | "shinyCharm";
export type ShopEntry = {
  kind: "item" | "egg";
  item: ItemKey | null;
  tier: Rarity | null;
  price: number;
  /** Items: how many the wallet affords (1 for an unowned passive item). */
  max_buy: number;
  can_buy: boolean;
};
export type Economy = {
  available: number;
  spent: number;
  shop: ShopEntry[];
  /** Bag: count > 0, in `ItemKind.allCases` order. */
  owned: { item: ItemKey; count: number; passive: boolean }[];
  candy_xp: number;
  /** Difficulty-scaled thresholds of the stages still ahead; empty when candy can't be used. */
  candy_costs: number[];
  max_candy_use: number;
  can_use_mint: boolean;
};

export const ITEM_SPRITE: Record<ItemKey, string | null> = { rareCandy: "rare-candy", mint: null, shinyCharm: "shiny-charm" };
export const ITEM_EMOJI: Record<ItemKey, string> = { rareCandy: "🍬", mint: "🌿", shinyCharm: "✨" };
export const itemSpriteUrl = (name: string) => `https://raw.githubusercontent.com/PokeAPI/sprites/master/sprites/items/${name}.png`;

/** Last view seen, so switching tabs does not flash an empty body while the fetch runs. */
let last: Economy | null = null;

/** Initial fetch plus `economy-changed` (wallet, inventory, prices follow companion and settings changes). */
export function useEconomy() {
  const [e, setE] = useState<Economy | null>(last);
  useEffect(() => {
    let live = true;
    const set = (v: Economy) => {
      last = v;
      if (live) setE(v);
    };
    invoke<Economy>("economy").then(set);
    const sub = listen<Economy>("economy-changed", (ev) => set(ev.payload));
    return () => {
      live = false;
      sub.then((f) => f());
    };
  }, []);
  return e;
}

export type CandyPlan = { count: number; evolves: boolean; graduates: boolean; carryoverXP: number; discardedXP: number };

/** `planRareCandyUse`: walk the stage costs with `used + count × xp`. Null when nothing can be used. */
export function planCandy(requested: number, e: Pick<Economy, "candy_costs" | "max_candy_use" | "candy_xp">, usedAtStage: number): CandyPlan | null {
  const count = Math.min(Math.max(0, requested), e.max_candy_use);
  if (count <= 0) return null;
  let remaining = usedAtStage + count * e.candy_xp;
  let evolves = false;
  for (let i = 0; i < e.candy_costs.length; i++) {
    const cost = e.candy_costs[i];
    if (remaining < cost) break;
    remaining -= cost;
    if (i === e.candy_costs.length - 1) return { count, evolves, graduates: true, carryoverXP: 0, discardedXP: remaining };
    evolves = true;
  }
  return { count, evolves, graduates: false, carryoverXP: evolves ? remaining : 0, discardedXP: 0 };
}
