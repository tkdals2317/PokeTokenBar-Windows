//! Growth balance: rarity, stage thresholds, difficulty (companion.md §3.1-3.2, Model:60-179).

use serde::{Deserialize, Serialize};

pub const EGG_HATCH_THRESHOLD: u64 = 5_000_000;
pub const REPEAT_GROWTH_MULTIPLIER: u64 = 2;
pub const SULK_AFTER_IDLE_SECS: u64 = 3 * 3600;
/// `SaveTransfer.maxTokenValue`: every token field is clamped to this.
pub const MAX_TOKEN_VALUE: u64 = 1_000_000_000_000_000;
pub const DIFFICULTY_RANGE: (f64, f64) = (0.1, 2.0);
pub const DEFAULT_DIFFICULTY: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Legendary,
}

impl Rarity {
    pub fn from(capture_rate: u32, legendary_or_mythical: bool) -> Self {
        match capture_rate {
            _ if legendary_or_mythical => Rarity::Legendary,
            0..=45 => Rarity::Rare,
            46..=120 => Rarity::Uncommon,
            _ => Rarity::Common,
        }
    }

    pub fn graduation_total(self) -> u64 {
        match self {
            Rarity::Common => 750_000_000,
            Rarity::Uncommon => 1_875_000_000,
            Rarity::Rare => 3_000_000_000,
            Rarity::Legendary => 6_000_000_000,
        }
    }

    /// Highest capture rate of the tier; `None` for legendary (not a capture-rate tier).
    pub fn capture_rate_ceiling(self) -> Option<u32> {
        match self {
            Rarity::Rare => Some(45),
            Rarity::Uncommon => Some(120),
            Rarity::Common => Some(255),
            Rarity::Legendary => None,
        }
    }

    /// Guaranteed-egg filter: "this tier or better" by capture rate.
    pub fn includes(self, capture_rate: u32) -> bool {
        self.capture_rate_ceiling().is_some_and(|c| capture_rate <= c)
    }

    pub fn sort_rank(self) -> u8 {
        self as u8
    }
}

/// A non-finite value becomes 1; anything else is clamped (0 and negatives become 0.1).
pub fn clamp_difficulty(d: f64) -> f64 {
    if d.is_finite() { d.clamp(DIFFICULTY_RANGE.0, DIFFICULTY_RANGE.1) } else { DEFAULT_DIFFICULTY }
}

/// `PokemonBalance.scaled`: no `max(1)` afterwards, like Swift.
pub fn scaled(base: u64, difficulty: f64) -> u64 {
    (base as f64 * clamp_difficulty(difficulty)).round() as u64
}

/// Tokens to leave stage `stage_index` (0-based) of a `k`-form line, before difficulty.
/// Rounded twice like Swift: the standard threshold, then the repeat-growth division.
pub fn phase_threshold(rarity: Rarity, k: usize, stage_index: usize, growth_multiplier: u64) -> u64 {
    let kk = k.max(1) as f64;
    let i = (stage_index + 1) as f64;
    let t = rarity.graduation_total() as f64;
    let standard = (t * i / (kk * (kk + 1.0) / 2.0)).round();
    ((standard / growth_multiplier.max(1) as f64).round() as u64).max(1)
}

/// Standard-balance growth for `completed` finished stages plus the current stage's usage.
pub fn reconstructed_growth(rarity: Rarity, total_forms: usize, completed: usize, current: u64) -> u64 {
    let forms = total_forms.max(1);
    let done: u64 = (0..completed.min(forms)).map(|s| phase_threshold(rarity, forms, s, 1)).sum();
    (done + current).min(rarity.graduation_total())
}

/// Keeps the earned fraction when a threshold base moves from `old` to `new` difficulty.
/// An incomplete stage never becomes complete by rounding.
pub fn rescale(credits: u64, base: u64, old: f64, new: f64) -> u64 {
    let old_thr = ((base as f64 * old).round() as u64).max(1);
    let new_thr = ((base as f64 * new).round() as u64).max(1);
    let v = (credits as f64 / old_thr as f64 * new_thr as f64).floor().clamp(0.0, MAX_TOKEN_VALUE as f64) as u64;
    if credits < old_thr { v.min(new_thr - 1) } else { v }
}

/// Settings slider position (0…1, log scale) → difficulty, snapping to 1.0 within ±0.01.
#[allow(dead_code)] // mirrored by the settings slider in TS; kept for the shared test vectors
pub fn difficulty_at_position(position: f64) -> f64 {
    let p = position.clamp(0.0, 1.0);
    if (p - difficulty_position(DEFAULT_DIFFICULTY)).abs() < 0.01 {
        return DEFAULT_DIFFICULTY;
    }
    snap(0.1 * 20f64.powf(p))
}

pub fn difficulty_position(v: f64) -> f64 {
    (clamp_difficulty(v) / 0.1).ln() / 20f64.ln()
}

/// Two significant figures; 0.1 for v ≤ 0.
pub fn snap(v: f64) -> f64 {
    if v <= 0.0 {
        return 0.1;
    }
    let m = 10f64.powf(v.log10().floor() - 1.0);
    (v / m).round() * m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thresholds_match_worked_examples() {
        let c3 = |s, m| phase_threshold(Rarity::Common, 3, s, m);
        assert_eq!((c3(0, 1), c3(1, 1), c3(2, 1)), (125_000_000, 250_000_000, 375_000_000));
        assert_eq!((c3(0, 2), c3(1, 2), c3(2, 2)), (62_500_000, 125_000_000, 187_500_000));
        assert_eq!(phase_threshold(Rarity::Legendary, 1, 0, 1), 6_000_000_000);
        assert_eq!(phase_threshold(Rarity::Rare, 2, 1, 1), 2_000_000_000);
        assert_eq!(phase_threshold(Rarity::Rare, 1, 0, 2), 1_500_000_000);
        let sum: u64 = (0..3).map(|i| phase_threshold(Rarity::Uncommon, 3, i, 1)).sum();
        assert_eq!(sum, 1_875_000_000);
    }

    #[test]
    fn rarity_tiers() {
        assert_eq!(Rarity::from(45, false), Rarity::Rare);
        assert_eq!(Rarity::from(120, false), Rarity::Uncommon);
        assert_eq!(Rarity::from(255, false), Rarity::Common);
        assert_eq!(Rarity::from(3, true), Rarity::Legendary);
        assert!(Rarity::Rare.includes(3) && !Rarity::Rare.includes(46));
        assert!(Rarity::Uncommon.includes(45) && !Rarity::Legendary.includes(3));
    }

    #[test]
    fn difficulty_clamp_and_scale() {
        assert_eq!(clamp_difficulty(0.0), 0.1);
        assert_eq!(clamp_difficulty(-3.0), 0.1);
        assert_eq!(clamp_difficulty(f64::NAN), 1.0);
        assert_eq!(clamp_difficulty(50.0), 2.0);
        assert_eq!(scaled(5_000_000, 0.1), 500_000);
        assert_eq!(scaled(1, 0.1), 0); // no max(1), like Swift
    }

    #[test]
    fn rescale_never_completes_a_stage() {
        // 99.99% at ×1 → ×2 would round to the threshold; it must stay one short.
        assert_eq!(rescale(4_999_999, 5_000_000, 1.0, 2.0), 9_999_998);
        assert_eq!(rescale(4_999_999, 5_000_000, 1.0, 0.1), 499_999);
        assert_eq!(rescale(1_000_000, 5_000_000, 1.0, 2.0), 2_000_000);
        assert_eq!(rescale(7_000_000, 5_000_000, 1.0, 0.5), 3_500_000); // already complete stays complete
        assert_eq!(rescale(333, 1_000, 1.0, 0.1), 33); // floor, not round
    }

    #[test]
    fn slider_mapping() {
        assert_eq!(difficulty_at_position(difficulty_position(1.0) + 0.005), 1.0);
        assert_eq!(difficulty_at_position(0.0), 0.1);
        assert!((difficulty_at_position(1.0) - 2.0).abs() < 1e-9);
        assert!((snap(0.1234) - 0.12).abs() < 1e-9);
        assert!((snap(1.467) - 1.5).abs() < 1e-9);
        assert_eq!(snap(0.0), 0.1);
        assert!((difficulty_position(2.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn reconstructed_growth_sums_completed_stages() {
        assert_eq!(reconstructed_growth(Rarity::Common, 3, 2, 0), 375_000_000);
        assert_eq!(reconstructed_growth(Rarity::Common, 3, 3, 10), 750_000_000);
    }
}
