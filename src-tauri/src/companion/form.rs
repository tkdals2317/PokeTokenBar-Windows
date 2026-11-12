//! Alternate forms of Gen 1–5 species (companion.md §3.5, Form:45-181).
//! The frontend has the same catalog in `src/areas/companion/forms.ts` for sprite names.

pub const UNOWN: u32 = 201;
/// Default looks with no official name; labelled "Normal Form".
pub const UNNAMED_DEFAULT: [u32; 4] = [351, 479, 646, 649];
const LINE_BASE_IDS: [(u32, u32); 5] = [(413, 412), (421, 420), (423, 422), (555, 554), (586, 585)];

/// (form, PokéAPI pokemon-form id). Default first.
type Entry = (&'static str, u32);

const UNOWN_LETTERS: [&str; 28] = [
    "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s",
    "t", "u", "v", "w", "x", "y", "z", "exclamation", "question",
];

const CATALOG: &[(u32, &[Entry])] = &[
    (351, &[("normal", 351), ("sunny", 10028), ("rainy", 10029), ("snowy", 10030)]),
    (386, &[("normal", 386), ("attack", 10031), ("defense", 10032), ("speed", 10033)]),
    (412, &[("plant", 412), ("sandy", 10034), ("trash", 10035)]),
    (413, &[("plant", 413), ("sandy", 10036), ("trash", 10037)]),
    (421, &[("overcast", 421), ("sunshine", 10038)]),
    (422, &[("west", 422), ("east", 10039)]),
    (423, &[("west", 423), ("east", 10040)]),
    (479, &[("normal", 479), ("heat", 10058), ("wash", 10059), ("frost", 10060), ("fan", 10061), ("mow", 10062)]),
    (487, &[("altered", 487), ("origin", 10063)]),
    (492, &[("land", 492), ("sky", 10064)]),
    (493, &[
        ("normal", 493), ("fighting", 10045), ("flying", 10047), ("poison", 10052), ("ground", 10050),
        ("rock", 10054), ("bug", 10041), ("ghost", 10048), ("steel", 10055), ("fire", 10046),
        ("water", 10056), ("grass", 10049), ("electric", 10044), ("psychic", 10053), ("ice", 10051),
        ("dragon", 10043), ("dark", 10042), ("fairy", 10085),
    ]),
    (550, &[("red-striped", 550), ("blue-striped", 10066)]),
    (555, &[("standard", 555), ("zen", 10067)]),
    (585, &[("spring", 585), ("summer", 10068), ("autumn", 10069), ("winter", 10070)]),
    (586, &[("spring", 586), ("summer", 10071), ("autumn", 10072), ("winter", 10073)]),
    (641, &[("incarnate", 641), ("therian", 10079)]),
    (642, &[("incarnate", 642), ("therian", 10080)]),
    (645, &[("incarnate", 645), ("therian", 10081)]),
    (646, &[("normal", 646), ("black", 10082), ("white", 10083)]),
    (647, &[("ordinary", 647), ("resolute", 10084)]),
    (648, &[("aria", 648), ("pirouette", 10074)]),
    (649, &[("normal", 649), ("douse", 10075), ("shock", 10076), ("burn", 10077), ("chill", 10078)]),
];

/// The species' forms with their `pokemon-form` ids, default first. Empty without forms.
pub fn entries(species: u32) -> Vec<(String, u32)> {
    if species == UNOWN {
        return UNOWN_LETTERS
            .iter()
            .enumerate()
            .map(|(i, l)| (l.to_string(), if i == 0 { UNOWN } else { 10000 + i as u32 }))
            .collect();
    }
    CATALOG
        .iter()
        .find(|(id, _)| *id == species)
        .map(|(_, e)| e.iter().map(|(f, id)| (f.to_string(), *id)).collect())
        .unwrap_or_default()
}

pub fn forms(species: u32) -> Vec<String> {
    entries(species).into_iter().map(|(f, _)| f).collect()
}

fn catalog_ids() -> impl Iterator<Item = u32> {
    std::iter::once(UNOWN).chain(CATALOG.iter().map(|(id, _)| *id))
}

/// Catalogued species whose line starts at `base`, sorted.
pub fn form_species(base: u32) -> Vec<u32> {
    let line_base = |id: u32| LINE_BASE_IDS.iter().find(|(s, _)| *s == id).map_or(id, |(_, b)| *b);
    let mut v: Vec<u32> = catalog_ids().filter(|id| line_base(*id) == base).collect();
    v.sort();
    v
}

/// Every form an individual of this line can carry, default first, without duplicates.
pub fn line_forms(base: u32) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for f in form_species(base).into_iter().flat_map(forms) {
        if !out.contains(&f) {
            out.push(f);
        }
    }
    out
}

/// The individual's look at `species`: its form if that species has it, else the default.
pub fn resolved(species: u32, form: Option<&str>) -> Option<String> {
    pick(forms(species), form)
}

/// Normalizes a stored form against the line: nil without forms, the default when unknown.
pub fn resolved_for_line(base: u32, form: Option<&str>) -> Option<String> {
    pick(line_forms(base), form)
}

fn pick(forms: Vec<String>, form: Option<&str>) -> Option<String> {
    let first = forms.first()?.clone();
    Some(form.filter(|f| forms.iter().any(|x| x == f)).map(str::to_string).unwrap_or(first))
}

pub fn is_default(species: u32, form: Option<&str>) -> bool {
    forms(species).first() == resolved(species, form).as_ref()
}

/// Uncollected forms weigh 2, collected 1; the remainder after `dropLast` goes to the last form.
pub fn roll(roll: u64, forms: &[String], collected: &[String]) -> String {
    let weights: Vec<u64> = forms.iter().map(|f| if collected.contains(f) { 1 } else { 2 }).collect();
    let total: u64 = weights.iter().sum();
    let mut remaining = (roll % total) as i64;
    for (f, w) in forms.iter().zip(&weights).take(forms.len() - 1) {
        remaining -= *w as i64;
        if remaining < 0 {
            return f.clone();
        }
    }
    forms.last().unwrap().clone()
}

/// Unown's letter: "A", "!" or "?".
pub fn symbol(form: &str) -> String {
    match form {
        "exclamation" => "!".into(),
        "question" => "?".into(),
        f => f.to_uppercase(),
    }
}

/// `pokemon-form` resource carrying the localized form name, if this look has one.
pub fn name_resource(species: u32, form: Option<&str>) -> Option<u32> {
    if species == UNOWN || (UNNAMED_DEFAULT.contains(&species) && is_default(species, form)) {
        return None;
    }
    let resolved = resolved(species, form)?;
    entries(species).into_iter().find(|(f, _)| *f == resolved).map(|(_, id)| id)
}

/// "Name [A]" for Unown, "Name [Label]" for a labelled non-default form, else the name.
pub fn display_name(name: &str, species: u32, form: Option<&str>, label: Option<&str>) -> String {
    let Some(resolved) = resolved(species, form) else { return name.to_string() };
    if species == UNOWN {
        return format!("{name} [{}]", symbol(&resolved));
    }
    match label {
        Some(l) if !l.is_empty() && !is_default(species, Some(&resolved)) => format!("{name} [{l}]"),
        _ => name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unown_has_28_letters() {
        let e = entries(UNOWN);
        assert_eq!(e.len(), 28);
        assert_eq!(e[0], ("a".into(), 201));
        assert_eq!(e[1], ("b".into(), 10001));
        assert_eq!(e[27], ("question".into(), 10027));
        assert_eq!(symbol("question"), "?");
        assert_eq!(display_name("Unown", UNOWN, None, None), "Unown [A]");
    }

    #[test]
    fn line_forms_resolve_per_species() {
        assert_eq!(form_species(412), vec![412, 413]);
        assert_eq!(line_forms(412), vec!["plant", "sandy", "trash"]);
        assert_eq!(resolved(413, Some("sandy")).as_deref(), Some("sandy"));
        assert_eq!(resolved(414, Some("sandy")), None); // Mothim has no forms
        assert_eq!(form_species(420), vec![421]); // Cherubi's line carries Cherrim's form
        assert_eq!(resolved_for_line(1, Some("x")), None);
        assert_eq!(resolved_for_line(201, Some("zz")).as_deref(), Some("a"));
        assert!(is_default(492, None) && !is_default(492, Some("sky")));
    }

    #[test]
    fn roll_favours_uncollected() {
        let forms = line_forms(422); // west, east
        let collected = vec!["west".to_string()];
        // weights west 1, east 2: rolls 0 → west, 1 and 2 → east
        assert_eq!(roll(0, &forms, &collected), "west");
        assert_eq!(roll(1, &forms, &collected), "east");
        assert_eq!(roll(2, &forms, &collected), "east");
        assert_eq!(roll(3, &forms, &collected), "west");
    }

    #[test]
    fn names_and_resources() {
        assert_eq!(name_resource(492, Some("sky")), Some(10064));
        assert_eq!(name_resource(479, None), None); // unnamed default
        assert_eq!(name_resource(479, Some("heat")), Some(10058));
        assert_eq!(display_name("Shaymin", 492, Some("sky"), Some("Sky Forme")), "Shaymin [Sky Forme]");
        assert_eq!(display_name("Shaymin", 492, None, Some("Land Forme")), "Shaymin");
    }
}
