//! Localized companion text produced in Rust: notifications, tray growth, names
//! (companion.md §3.12-3.13, §5.7; Localization.swift:1359-1445). Order: ko en ja es fr pt de ru.

use super::balance::Rarity;
use std::collections::HashMap;

fn t<'a>(lang: &str, v: [&'a str; 8]) -> &'a str {
    let i = ["ko", "en", "ja", "es", "fr", "pt", "de", "ru"].iter().position(|l| *l == lang).unwrap_or(1);
    v[i]
}

/// Korean particle after a runtime name (KoreanParticle.swift); non-Hangul keeps the dual form.
#[derive(Clone, Copy)]
pub enum Particle {
    Subject,
    Direction,
}

pub fn particle(word: &str, p: Particle) -> String {
    let coda = word.chars().last().map(|c| c as u32).filter(|c| (0xAC00..=0xD7A3).contains(c)).map(|c| (c - 0xAC00) % 28);
    let suffix = match (p, coda) {
        (Particle::Subject, None) => "이(가)",
        (Particle::Subject, Some(0)) => "가",
        (Particle::Subject, Some(_)) => "이",
        (Particle::Direction, None) => "(으)로",
        (Particle::Direction, Some(0 | 8)) => "로",
        (Particle::Direction, Some(_)) => "으로",
    };
    format!("{word}{suffix}")
}

pub fn hatch_title(lang: &str, shiny: bool) -> String {
    if shiny {
        t(lang, ["✨ 이로치 포켓몬!", "✨ Shiny Pokémon!", "✨ 色違いポケモン！", "✨ ¡Pokémon variocolor!", "✨ Pokémon chromatique !", "✨ Pokémon shiny!", "✨ Schillerndes Pokémon!", "✨ Шайни-покемон!"]).into()
    } else {
        t(lang, ["🥚 부화!", "🥚 Hatched!", "🥚 孵化！", "🥚 ¡Eclosionó!", "🥚 Éclosion !", "🥚 Chocou!", "🥚 Geschlüpft!", "🥚 Вылупился!"]).into()
    }
}

pub fn hatch_body(lang: &str, n: &str) -> String {
    match lang {
        "ko" => format!("알에서 {} 나왔어요!", particle(n, Particle::Subject)),
        "ja" => format!("タマゴから {n} が生まれました！"),
        "es" => format!("¡{n} salió del huevo!"),
        "fr" => format!("{n} est sorti de l'œuf !"),
        "pt" => format!("{n} saiu do ovo!"),
        "de" => format!("{n} ist aus dem Ei geschlüpft!"),
        "ru" => format!("Из яйца вылупился {n}!"),
        _ => format!("{n} hatched from the egg!"),
    }
}

pub fn shiny_hatch_body(lang: &str, n: &str, odds: u64) -> String {
    match lang {
        "ko" => format!("이로치 {} 태어났어요! (1/{odds})", particle(n, Particle::Subject)),
        "ja" => format!("色違いの {n} が生まれました！(1/{odds})"),
        "es" => format!("¡Nació un {n} variocolor! (1 entre {odds})"),
        "fr" => format!("Un {n} chromatique est né ! (1 sur {odds})"),
        "pt" => format!("Nasceu um {n} shiny! (1 em {odds})"),
        "de" => format!("Ein schillerndes {n} ist geschlüpft! (1/{odds})"),
        "ru" => format!("Вылупился шайни {n}! (шанс 1 к {odds})"),
        _ => format!("A shiny {n} hatched! (1 in {odds})"),
    }
}

/// Hatch body once the egg was prayed over; the joke follows the outcome.
pub fn prayer_hatch_body(lang: &str, n: &str, prayers: u32, shiny: bool, rarity: Rarity) -> String {
    let p = prayers;
    let sub = particle(n, Particle::Subject);
    if shiny && rarity == Rarity::Legendary {
        return match lang {
            "ko" => format!("기도 {p}번… 하늘이 진심이었다. 이로치 {n}!"),
            "ja" => format!("お祈り{p}回… 天も本気だった。色違いの{n}！"),
            "es" => format!("¡{p} oraciones… el cielo fue con todo. {n} variocolor!"),
            "fr" => format!("{p} prières… le ciel a tout donné. {n} chromatique !"),
            "pt" => format!("{p} orações… o céu foi com tudo. {n} shiny!"),
            "de" => format!("{p} Gebete… der Himmel hat alles gegeben. Schillerndes {n}!"),
            "ru" => format!("{p} молитв… небеса постарались на все сто. Шайни {n}!"),
            _ => format!("{p} prayers… heaven went all in. Shiny {n}!"),
        };
    }
    if rarity == Rarity::Legendary {
        return match lang {
            "ko" => format!("기도 {p}번… 통했다?! {sub} 나왔어요!"),
            "ja" => format!("お祈り{p}回… 通じた？！{n} が生まれました！"),
            "es" => format!("{p} oraciones… ¿funcionó?! ¡Salió {n}!"),
            "fr" => format!("{p} prières… ça a marché ?! {n} est sorti !"),
            "pt" => format!("{p} orações… funcionou?! Saiu {n}!"),
            "de" => format!("{p} Gebete… hat es gewirkt?! {n} ist geschlüpft!"),
            "ru" => format!("{p} молитв… сработало?! Вылупился {n}!"),
            _ => format!("{p} prayers… it worked?! {n} hatched!"),
        };
    }
    if shiny {
        return match lang {
            "ko" => format!("기도 {p}번에 이로치까지?! {sub} 나왔어요!"),
            "ja" => format!("お祈り{p}回で色違いまで？！{n} が生まれました！"),
            "es" => format!("¿{p} oraciones Y encima variocolor?! ¡Salió {n}!"),
            "fr" => format!("{p} prières ET en plus chromatique ?! {n} est sorti !"),
            "pt" => format!("{p} orações E ainda shiny?! Saiu {n}!"),
            "de" => format!("{p} Gebete UND dann noch schillernd?! {n} ist geschlüpft!"),
            "ru" => format!("{p} молитв И ещё шайни?! Вылупился {n}!"),
            _ => format!("{p} prayers AND it's shiny?! {n} hatched!"),
        };
    }
    if rarity == Rarity::Common {
        return match lang {
            "ko" => format!("기도 {p}번 끝에 {sub} 나왔어요. 하늘도 오늘 바빴나 봐요."),
            "ja" => format!("お祈り{p}回の末、{n} が生まれました。天国も今日は忙しかったみたい。"),
            "es" => format!("Tras {p} oraciones: {n}. El cielo debe de andar saturado hoy."),
            "fr" => format!("Après {p} prières : {n}. Le ciel doit être débordé aujourd'hui."),
            "pt" => format!("Após {p} orações: {n}. O céu deve estar lotado hoje."),
            "de" => format!("Nach {p} Gebeten: {n}. Der Himmel hat heute wohl viel zu tun."),
            "ru" => format!("После {p} молитв: {n}. У небес сегодня, видимо, много дел."),
            _ => format!("{p} prayers later: {n}. Heaven must be swamped today."),
        };
    }
    match lang {
        "ko" => format!("기도 {p}번 끝에 {sub} 나왔어요. 반은 통한 걸로 칩시다."),
        "ja" => format!("お祈り{p}回の末、{n} が生まれました。半分は通じたってことにしよう。"),
        "es" => format!("Tras {p} oraciones: {n}. Digamos que fue medio milagro."),
        "fr" => format!("Après {p} prières : {n}. Disons que c'est un demi-miracle."),
        "pt" => format!("Após {p} orações: {n}. Vamos chamar de meio milagre."),
        "de" => format!("Nach {p} Gebeten: {n}. Nennen wir es ein halbes Wunder."),
        "ru" => format!("После {p} молитв: {n}. Считаем это половиной чуда."),
        _ => format!("{p} prayers later: {n}. Let's call it half a miracle."),
    }
}

pub fn evolve_title(lang: &str) -> String {
    t(lang, ["✨ 진화!", "✨ Evolved!", "✨ 進化！", "✨ ¡Evolucionó!", "✨ Évolution !", "✨ Evoluiu!", "✨ Entwicklung!", "✨ Эволюция!"]).into()
}

pub fn evolve_body(lang: &str, old: &str, new: &str) -> String {
    match lang {
        "ko" => format!("{} {} 진화했어요!", particle(old, Particle::Subject), particle(new, Particle::Direction)),
        "ja" => format!("{old} が {new} に進化しました！"),
        "es" => format!("¡{old} evolucionó a {new}!"),
        "fr" => format!("{old} a évolué en {new} !"),
        "pt" => format!("{old} evoluiu para {new}!"),
        "de" => format!("{old} hat sich zu {new} entwickelt!"),
        "ru" => format!("{old} эволюционировал в {new}!"),
        _ => format!("{old} evolved into {new}!"),
    }
}

pub fn ditto_title(lang: &str, shiny: bool) -> String {
    if shiny {
        t(lang, ["🎭✨ 어라? 이로치 메타몽!", "🎭✨ Huh? A shiny Ditto!", "🎭✨ あれ？色違いメタモン！", "🎭✨ ¿Eh? ¡Un Ditto variocolor!", "🎭✨ Hein ? Un Métamorph chromatique !", "🎭✨ Ué? Um Ditto shiny!", "🎭✨ Huch? Ein schillerndes Ditto!", "🎭✨ Что? Шайни-Ditto!"]).into()
    } else {
        t(lang, ["🎭 어라? 메타몽!", "🎭 Huh? It's Ditto!", "🎭 あれ？メタモン！", "🎭 ¿Eh? ¡Es Ditto!", "🎭 Hein ? C'est Métamorph !", "🎭 Ué? É um Ditto!", "🎭 Huch? Ditto!", "🎭 Что? Это Ditto!"]).into()
    }
}

pub fn ditto_body(lang: &str, d: &str, shiny: bool) -> String {
    if shiny {
        return match lang {
            "ko" => format!("{d}인 줄 알았는데 — 이로치 메타몽이었어요!"),
            "ja" => format!("{d} だと思ってた… 色違いのメタモンでした！"),
            "es" => format!("Pensabas que era {d} — ¡era un Ditto variocolor!"),
            "fr" => format!("Tu croyais que c'était {d} — c'était un Métamorph chromatique !"),
            "pt" => format!("Você achava que era {d} — era um Ditto shiny!"),
            "de" => format!("Du dachtest, es wäre {d} – dabei war es ein schillerndes Ditto!"),
            "ru" => format!("Думали, это {d}? А это шайни-Ditto!"),
            _ => format!("You thought it was {d} — it was a shiny Ditto!"),
        };
    }
    match lang {
        "ko" => format!("{d}인 줄 알았는데 — 사실은 메타몽이었어요!"),
        "ja" => format!("{d} だと思ってた… 実はメタモンでした！"),
        "es" => format!("Pensabas que era {d} — ¡en realidad era Ditto!"),
        "fr" => format!("Tu croyais que c'était {d} — c'était Métamorph depuis le début !"),
        "pt" => format!("Você achava que era {d} — era um Ditto o tempo todo!"),
        "de" => format!("Du dachtest, es wäre {d} – dabei war es die ganze Zeit Ditto!"),
        "ru" => format!("Думали, это {d}? А это всё время был Ditto!"),
        _ => format!("You thought it was {d} — it was Ditto all along!"),
    }
}

pub fn graduate_title(lang: &str) -> String {
    t(lang, ["🎓 졸업!", "🎓 Graduated!", "🎓 卒業！", "🎓 ¡Graduado!", "🎓 Diplômé !", "🎓 Formatura!", "🎓 Abschied!", "🎓 Выпуск!"]).into()
}

pub fn graduate_body(lang: &str, n: &str) -> String {
    match lang {
        "ko" => format!("{n} — 도감에 보존! 새 알이 도착했어요."),
        "ja" => format!("{n} — 図鑑に保存！新しいタマゴが届きました。"),
        "es" => format!("{n} — ¡guardado en tu Pokédex! Ha llegado un nuevo huevo."),
        "fr" => format!("{n} — conservé dans ton Pokédex ! Un nouvel œuf est arrivé."),
        "pt" => format!("{n} — guardado na sua Pokédex! Chegou um novo ovo."),
        "de" => format!("{n} – in deinem Pokédex gespeichert! Ein neues Ei ist da."),
        "ru" => format!("{n} — запись в Покедексе! Появилось новое яйцо."),
        _ => format!("{n} — saved to your Pokédex! A new egg has arrived."),
    }
}

/// `menuGrowthProgress`: "Hatch N%" / "Growth N%".
pub fn menu_growth(lang: &str, pct: u64, egg: bool) -> String {
    let w = if egg {
        t(lang, ["부화", "Hatch", "孵化", "Eclosión", "Éclosion", "Eclosão", "Schlüpfen", "Вылупление"])
    } else {
        t(lang, ["성장", "Growth", "成長", "Crecimiento", "Croissance", "Crescimento", "Wachstum", "Рост"])
    };
    format!("{w} {pct}%")
}

/// PokéAPI codes tried for an app language, before the `en` fallback.
pub fn api_codes(lang: &str) -> &'static [&'static str] {
    match lang {
        "ko" => &["ko"],
        "ja" => &["ja-hrkt", "ja"],
        "es" => &["es"],
        "fr" => &["fr"],
        "pt" => &["pt-br", "pt"],
        "de" => &["de"],
        "ru" => &["ru"],
        _ => &["en"],
    }
}

/// Language code normalization: trim, lowercase, `_` → `-`.
pub fn lang_code(raw: &str) -> String {
    raw.trim().to_lowercase().replace('_', "-")
}

/// `PokemonNameLocalization.resolve`: app language codes, then `en`.
pub fn resolve_name(names: &HashMap<String, String>, lang: &str) -> Option<String> {
    let norm: HashMap<String, &str> =
        names.iter().filter(|(_, v)| !v.trim().is_empty()).map(|(k, v)| (lang_code(k), v.trim())).collect();
    api_codes(lang).iter().chain(["en"].iter()).find_map(|c| norm.get(*c).map(|s| s.to_string()))
}

/// "sky-forme" → "Sky Forme": the offline fallback for form, type, move and ability names.
pub fn identifier(raw: &str) -> String {
    raw.split('-')
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn korean_particles() {
        assert_eq!(particle("피카츄", Particle::Subject), "피카츄가");
        assert_eq!(particle("이상해씨", Particle::Subject), "이상해씨가");
        assert_eq!(particle("리자몽", Particle::Subject), "리자몽이");
        assert_eq!(particle("리자몽", Particle::Direction), "리자몽으로");
        assert_eq!(particle("알", Particle::Direction), "알로");
        assert_eq!(particle("#25", Particle::Direction), "#25(으)로");
        assert_eq!(evolve_body("ko", "파이리", "리자드"), "파이리가 리자드로 진화했어요!");
    }

    #[test]
    fn names_resolve_with_fallbacks() {
        let n = HashMap::from([("ja-Hrkt".to_string(), "ピカチュウ".to_string()), ("en".into(), "Pikachu".into())]);
        assert_eq!(resolve_name(&n, "ja").as_deref(), Some("ピカチュウ"));
        assert_eq!(resolve_name(&n, "ru").as_deref(), Some("Pikachu"));
        assert_eq!(resolve_name(&HashMap::new(), "ko"), None);
        assert_eq!(identifier("sky-forme"), "Sky Forme");
        assert_eq!(menu_growth("de", 42, true), "Schlüpfen 42%");
    }
}
