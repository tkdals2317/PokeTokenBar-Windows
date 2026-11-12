//! Localized economy text produced in Rust: candy grant and dex completion notifications,
//! limit window names (Localization.swift:93-102, 1447-1542, 1825, 1960-1980). Order: ko en ja es fr pt de ru.

fn t<'a>(lang: &str, v: [&'a str; 8]) -> &'a str {
    let i = ["ko", "en", "ja", "es", "fr", "pt", "de", "ru"].iter().position(|l| *l == lang).unwrap_or(1);
    v[i]
}

/// `t` with `{}` placeholders filled in order.
fn tf(lang: &str, v: [&str; 8], args: &[&str]) -> String {
    args.iter().fold(t(lang, v).to_string(), |s, a| s.replacen("{}", a, 1))
}

fn rare_candy(lang: &str) -> &'static str {
    t(lang, ["이상한 사탕", "Rare Candy", "ふしぎなアメ", "Caramelo Raro", "Super Bonbon", "Doce Raro", "Sonderbonbon", "Редкая конфета"])
}

pub fn candy_title(lang: &str, count: u32) -> String {
    let (item, n) = (rare_candy(lang), count.to_string());
    match lang {
        "ko" => format!("🍬 {item} {n}개를 받았어요!"),
        "ja" => format!("🍬 {item}を{n}個もらいました！"),
        "es" => format!("🍬 ¡Has recibido {n}× {item}!"),
        "fr" => format!("🍬 Tu as reçu {n}× {item} !"),
        "pt" => format!("🍬 Você ganhou {n}× {item}!"),
        "de" => format!("🍬 Du hast {n}× {item} erhalten!"),
        "ru" => format!("🍬 Вы получили: {item} ×{n}!"),
        _ => format!("🍬 You got {n}× {item}!"),
    }
}

pub fn candy_body(lang: &str, window: &str) -> String {
    tf(
        lang,
        [
            "{} 토큰 한도를 다 채웠어요. 열심히 쓴 만큼 사탕을 드려요 — 포켓몬에게 써서 진화시켜 보세요!",
            "You maxed out your {} token limit. A treat for the effort — use it to evolve your Pokémon!",
            "{}のトークン上限を使い切りました。がんばったごほうびです — ポケモンに使って進化させよう！",
            "Has agotado tu límite de tokens {}. Un premio por el esfuerzo — ¡úsalo para evolucionar a tu Pokémon!",
            "Tu as atteint ta limite de tokens {}. Une récompense pour l'effort — utilise-la pour faire évoluer ton Pokémon !",
            "Você esgotou seu limite de tokens — {}. Você merece um agrado: use no seu Pokémon para evoluir!",
            "Du hast das Token-Limit für {} ausgeschöpft. Eine Belohnung für deinen Einsatz – verwende sie, um dein Pokémon zu entwickeln!",
            "Вы полностью исчерпали лимит токенов ({}). Это награда за старания — используйте её, чтобы эволюционировать покемона!",
        ],
        &[window],
    )
}

pub fn region_name(lang: &str, region: &str) -> &'static str {
    match region {
        "kanto" => t(lang, ["관동", "Kanto", "カントー", "Kanto", "Kanto", "Kanto", "Kanto", "Канто"]),
        "johto" => t(lang, ["성도", "Johto", "ジョウト", "Johto", "Johto", "Johto", "Johto", "Джото"]),
        "hoenn" => t(lang, ["호연", "Hoenn", "ホウエン", "Hoenn", "Hoenn", "Hoenn", "Hoenn", "Хоэнн"]),
        "sinnoh" => t(lang, ["신오", "Sinnoh", "シンオウ", "Sinnoh", "Sinnoh", "Sinnoh", "Sinnoh", "Синно"]),
        _ => t(lang, ["하나", "Unova", "イッシュ", "Teselia", "Unys", "Unova", "Einall", "Юнова"]),
    }
}

/// (title, body) for a badge key from `rules::earned_badges`.
pub fn badge_notification(lang: &str, key: &str) -> (String, String) {
    if let Some(r) = key.strip_prefix("region.") {
        let r = region_name(lang, r);
        return (
            tf(lang, ["🏅 {} 도감 완성!", "🏅 {} Dex complete!", "🏅 {}図鑑コンプリート！", "🏅 ¡Pokédex de {} completada!", "🏅 Pokédex de {} terminé !", "🏅 Pokédex de {} completa!", "🏅 {}-Dex komplett!", "🏅 Покедекс {} завершён!"], &[r]),
            tf(lang, ["{} 지방의 모든 포켓몬을 모았어요!", "You've caught every Pokémon in {}!", "{}地方のすべてのポケモンを集めました！", "¡Has capturado todos los Pokémon de {}!", "Tu as capturé tous les Pokémon de {} !", "Você capturou todos os Pokémon de {}!", "Du hast alle Pokémon aus {} gefangen!", "Вы поймали всех покемонов региона {}!"], &[r]),
        );
    }
    if let Some(r) = key.strip_prefix("regionShiny.") {
        let r = region_name(lang, r);
        return (
            tf(lang, ["✨ {} 이로치 도감 완성!", "✨ Shiny {} Dex complete!", "✨ {} 色違い図鑑コンプリート！", "✨ ¡Pokédex variocolor de {} completada!", "✨ Pokédex chromatique de {} terminé !", "✨ Pokédex shiny de {} completa!", "✨ Schillernde {}-Dex komplett!", "✨ Шайни-Покедекс {} завершён!"], &[r]),
            tf(lang, ["{} 지방의 모든 포켓몬을 이로치로 모았어요!", "You've caught a shiny of every Pokémon in {}!", "{}地方のすべてのポケモンを色違いで集めました！", "¡Has capturado un variocolor de cada Pokémon de {}!", "Tu as capturé un chromatique de chaque Pokémon de {} !", "Você capturou um shiny de cada Pokémon de {}!", "Du hast ein schillerndes Exemplar jedes Pokémon aus {}!", "Вы поймали шайни каждого покемона региона {}!"], &[r]),
        );
    }
    let (title, body) = match key {
        "unown" => (
            ["🔤 안농 도감 완성!", "🔤 Unown collection complete!", "🔤 アンノーン図鑑コンプリート！", "🔤 ¡Colección Unown completada!", "🔤 Collection Zarbi terminée !", "🔤 Coleção Unown completa!", "🔤 Icognito-Sammlung komplett!", "🔤 Коллекция Анноун завершена!"],
            ["28가지 안농 형태를 모두 모았어요!", "You've collected all 28 Unown forms!", "28種類のアンノーンをすべて集めました！", "¡Has reunido las 28 formas de Unown!", "Tu as réuni les 28 formes de Zarbi !", "Você reuniu todas as 28 formas de Unown!", "Du hast alle 28 Icognito-Formen gesammelt!", "Вы собрали все 28 форм Анноуна!"],
        ),
        "national" => (
            ["🌍 전국도감 완성!", "🌍 National Dex complete!", "🌍 全国図鑑コンプリート！", "🌍 ¡Pokédex Nacional completada!", "🌍 Pokédex national terminé !", "🌍 Pokédex Nacional completa!", "🌍 National-Dex komplett!", "🌍 Национальный Покедекс завершён!"],
            ["1~5세대 모든 포켓몬을 모았어요!", "You've caught every Pokémon from Gen 1–5!", "第1〜5世代のすべてのポケモンを集めました！", "¡Has capturado todos los Pokémon de la 1.ª a la 5.ª generación!", "Tu as capturé tous les Pokémon des générations 1 à 5 !", "Você capturou todos os Pokémon das gerações 1 a 5!", "Du hast alle Pokémon der Generationen 1–5 gefangen!", "Вы поймали всех покемонов 1–5 поколений!"],
        ),
        _ => (
            ["✨ 전국 이로치 도감 완성!", "✨ Shiny National Dex complete!", "✨ 色違い全国図鑑コンプリート！", "✨ ¡Pokédex Nacional variocolor completada!", "✨ Pokédex national chromatique terminé !", "✨ Pokédex Nacional shiny completa!", "✨ Schillernde National-Dex komplett!", "✨ Шайни Национальный Покедекс завершён!"],
            ["모든 포켓몬의 이로치를 모았어요!", "You've caught a shiny of every Pokémon!", "すべてのポケモンの色違いを集めました！", "¡Has capturado un variocolor de cada Pokémon!", "Tu as capturé un chromatique de chaque Pokémon !", "Você capturou um shiny de cada Pokémon!", "Du hast ein schillerndes Exemplar jedes Pokémon!", "Вы поймали шайни каждого покемона!"],
        ),
    };
    (t(lang, title).into(), t(lang, body).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_texts() {
        assert_eq!(candy_title("en", 5), "🍬 You got 5× Rare Candy!");
        assert_eq!(candy_title("ko", 1), "🍬 이상한 사탕 1개를 받았어요!");
        assert!(candy_body("en", "Claude weekly").starts_with("You maxed out your Claude weekly token limit."));
        assert_eq!(badge_notification("ko", "region.johto").0, "🏅 성도 도감 완성!");
        assert_eq!(badge_notification("en", "regionShiny.unova").0, "✨ Shiny Unova Dex complete!");
        assert_eq!(badge_notification("en", "nationalShiny").1, "You've caught a shiny of every Pokémon!");
    }
}
