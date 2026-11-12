//! Calls the shop-bag-dex area (economy) makes into companion state. Owner: companion area.
//! Signatures are the contract (shop-bag-dex.md §3.4-3.5, companion.md §3.3, §3.7).
//! Call them inside `companion::update`, which saves, notifies and emits afterwards.

use super::{Event, Rarity, State};

#[allow(dead_code)] // called by economy once it lands
impl State {
    /// Growth tokens credited since install: the wallet's income side.
    pub fn lifetime_tokens(&self) -> u64 {
        self.used_since_install
    }

    /// Rare Candy: feeds `xp` through the normal growth path (evolve / graduate / Ditto reveal),
    /// never `used_since_install`, never scaled by difficulty. Shows "+XP" on the card.
    /// Returns the `companion-event`s it caused (they are also emitted by `update`).
    pub fn apply_candy_xp(&mut self, xp: u64, rng: &mut fastrand::Rng) -> Vec<Event> {
        let before = self.rt.events.len();
        self.candy_xp(xp, rng);
        self.rt.events[before..].to_vec()
    }

    /// Mint: rerolls the active Pokémon's nature to a different one. `false` without an active Pokémon.
    pub fn reroll_nature(&mut self, rng: &mut fastrand::Rng) -> bool {
        self.mint_nature(rng)
    }

    /// Egg purchase: keeps the active Pokémon in the dex as a released record (reached forms only,
    /// `collected_finals` untouched) and restarts the egg with an optional rarity guarantee
    /// (`None`, uncommon or rare). `false` without an active Pokémon or for a legendary guarantee.
    pub fn release_active_for_egg(&mut self, guarantee: Option<Rarity>) -> bool {
        self.release_for_egg(guarantee)
    }
}
