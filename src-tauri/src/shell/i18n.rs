//! Tray menu strings in the 8 app languages (macOS Localization.swift). Webview strings live in
//! `src/shell/strings.ts` and the per-area dictionaries.

/// ko, en, ja, es, fr, pt, de, ru
type T = [&'static str; 8];

pub const TODAY_TOKENS: T = ["오늘 사용한 토큰", "Today's tokens", "本日のトークン", "Tokens de hoy", "Tokens du jour", "Tokens de hoje", "Heute verbrauchte Tokens", "Токены за сегодня"];
pub const REFRESH_NOW: T = ["지금 새로고침", "Refresh now", "今すぐ更新", "Actualizar ahora", "Actualiser maintenant", "Atualizar agora", "Jetzt aktualisieren", "Обновить сейчас"];
pub const DEX: T = ["도감", "Pokédex", "図鑑", "Pokédex", "Pokédex", "Pokédex", "Pokédex", "Покедекс"];
pub const SETTINGS: T = ["설정", "Settings", "設定", "Ajustes", "Réglages", "Ajustes", "Einstellungen", "Настройки"];
pub const HIDE_PET: T = ["플로팅 펫 숨기기", "Hide floating pet", "フローティングペットを隠す", "Ocultar mascota flotante", "Masquer le compagnon flottant", "Ocultar mascote flutuante", "Schwebendes Pokémon ausblenden", "Скрыть питомца"];
pub const SHOW_PET: T = ["플로팅 펫 표시", "Show floating pet", "フローティングペットを表示", "Mostrar mascota flotante", "Afficher le compagnon flottant", "Mostrar mascote flutuante", "Schwebendes Pokémon anzeigen", "Показывать питомца на экране"];
pub const BACK_VIEW: T = ["뒷모습 보기", "Show back view", "後ろ姿を見る", "Ver de espaldas", "Voir de dos", "Ver de costas", "Rückansicht zeigen", "Показывать спину"];
pub const QUIT: T = ["종료", "Quit", "終了", "Salir", "Quitter", "Encerrar", "Beenden", "Завершить"];
pub const COST_UNAVAILABLE: T = ["계산 불가", "Unavailable", "計算不可", "No disponible", "Indisponible", "Indisponível", "Nicht verfügbar", "Недоступно"];

pub fn tr(t: &T, lang: &str) -> &'static str {
    let i = crate::settings::LANGUAGES.iter().position(|l| *l == lang).unwrap_or(1);
    t[i]
}
