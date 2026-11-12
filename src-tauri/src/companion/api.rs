//! Calls the shop-bag-dex area (economy) makes into companion state. Owner: companion area.
//! Signatures are the contract; bodies are stubs until the companion area lands them
//! (shop-bag-dex.md §3.4-3.5, companion.md §3.6-3.7). Call them inside `companion::update`.

use super::{Event, Rarity, State};

#[allow(dead_code)] // called by economy once it lands
impl State {
    /// Growth tokens credited since install: the wallet's income side.
    pub fn lifetime_tokens(&self) -> u64 {
        self.used_since_install
    }

    /// Rare Candy: feeds `xp` through the normal growth path (evolve / graduate). Not wallet income.
    pub fn apply_candy_xp(&mut self, xp: u64, rng: &mut fastrand::Rng) -> Vec<Event> {
        self.feed(xp, rng)
    }

    /// Mint: rerolls the active Pokémon's nature to a different one. `false` without an active Pokémon.
    pub fn reroll_nature(&mut self, _rng: &mut fastrand::Rng) -> bool {
        todo!("companion area: shop-bag-dex.md §3.5 Mint")
    }

    /// Egg purchase: keeps the active Pokémon in the dex as a released record, restarts the egg
    /// with an optional rarity guarantee.
    pub fn release_active_for_egg(&mut self, _guarantee: Option<Rarity>) -> bool {
        todo!("companion area: shop-bag-dex.md §3.4 buyEgg")
    }
}
