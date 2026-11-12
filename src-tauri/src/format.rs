//! Literal port of macOS `TokenFormatter` for the tray tooltip and menu.
//! The webview has the same rules in `src/kit/format.ts`; keep both in sync.

use crate::model::CostCoverage;

fn trim(s: String) -> String {
    if !s.contains('.') {
        return s;
    }
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// 987, 12.3K, 1K, 190.6M, 1.24B. 999_950 promotes to the next unit so "1000.0K" never shows.
pub fn compact(value: i64) -> String {
    let v = value.unsigned_abs() as f64;
    let sign = if value < 0 { "-" } else { "" };
    if v < 1_000.0 {
        return value.to_string();
    }
    let (n, unit, decimals) = if v < 999_950.0 {
        (v / 1e3, "K", 1)
    } else if v < 999_950_000.0 {
        (v / 1e6, "M", 1)
    } else {
        (v / 1e9, "B", 2)
    };
    format!("{sign}{}{unit}", trim(format!("{n:.decimals$}")))
}

/// 80%, 88.3%
pub fn percent(v: f64) -> String {
    format!("{}%", format!("{v:.1}").trim_end_matches(".0"))
}

/// $2.76
pub fn cost(usd: f64) -> String {
    format!("${usd:.2}")
}

/// $9.5, $311, $12.3K. Thresholds are judged on the rounded text, like the original.
pub fn cost_compact(usd: f64) -> String {
    let tenths = format!("{usd:.1}");
    if tenths.parse::<f64>().unwrap_or(0.0) < 100.0 {
        return format!("${tenths}");
    }
    let whole = format!("{usd:.0}");
    if whole.parse::<f64>().unwrap_or(0.0) < 10_000.0 {
        return format!("${whole}");
    }
    format!("${:.1}K", usd / 1000.0)
}

/// `UsageCost.text`: unknown-only coverage shows `$—` (compact) or the localized "Unavailable".
pub fn cost_text(usd: f64, coverage: CostCoverage, compact: bool, unavailable: &str) -> String {
    let known = coverage.reported || coverage.estimated;
    match (coverage.unknown && !known, compact) {
        (true, true) => "$—".into(),
        (true, false) => unavailable.into(),
        (false, true) => cost_compact(usd),
        (false, false) => cost(usd),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_matches_token_formatter() {
        let cases = [
            (0, "0"),
            (999, "999"),
            (-999, "-999"),
            (987, "987"),
            (12_345, "12.3K"),
            (1_000, "1K"),
            (999_949, "999.9K"),
            (999_950, "1M"),
            (1_050_000, "1.1M"),
            (190_612_940, "190.6M"),
            (999_949_999, "999.9M"),
            (999_950_000, "1B"),
            (1_240_000_000, "1.24B"),
            (2_000_000_000, "2B"),
            (-12_345, "-12.3K"),
            (-999_950, "-1M"),
        ];
        for (v, want) in cases {
            assert_eq!(compact(v), want, "{v}");
        }
    }

    #[test]
    fn percent_and_costs() {
        assert_eq!(
            [percent(79.96), percent(88.35), percent(80.0), percent(0.0), percent(100.0)],
            ["80%", "88.3%", "80%", "0%", "100%"]
        );
        let compact_costs = [0.0, 9.5, 99.94, 99.96, 311.0, 9_999.4, 9_999.6, 12_345.0].map(cost_compact);
        assert_eq!(compact_costs, ["$0.0", "$9.5", "$99.9", "$100", "$311", "$9999", "$10.0K", "$12.3K"]);
        let unknown = CostCoverage { unknown: true, ..Default::default() };
        assert_eq!(cost_text(1.0, unknown, true, "Unavailable"), "$—");
        assert_eq!(cost_text(1.0, unknown, false, "Unavailable"), "Unavailable");
        let partial = CostCoverage { unknown: true, estimated: true, ..Default::default() };
        assert_eq!(cost_text(1.5, partial, true, "Unavailable"), "$1.5");
        assert_eq!(cost_text(2.756, CostCoverage::default(), false, ""), "$2.76");
    }
}
