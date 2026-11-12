// Home usage strings, verbatim from macOS Localization.swift (rows are ko, en, ja, es, fr, pt, de, ru).
// Owner: home-usage area.
import { defineStrings, LANGS, Strings } from "../../kit/i18n";

type Eight<T> = readonly [T, T, T, T, T, T, T, T];
const t = (...v: Eight<string>) => v;
const f = <A extends unknown[]>(...v: Eight<(...a: A) => string>) => v;

const rows = {
  todayTokens: t("오늘 사용한 토큰", "Today's tokens", "本日のトークン", "Tokens de hoy", "Tokens du jour", "Tokens de hoje", "Heute verbrauchte Tokens", "Токены за сегодня"),
  thisWeek: t("이번 주", "This week", "今週", "Esta semana", "Cette semaine", "Esta semana", "Diese Woche", "Эта неделя"),
  thisMonth: t("이번 달", "This month", "今月", "Este mes", "Ce mois-ci", "Este mês", "Dieser Monat", "Этот месяц"),
  thisYear: t("올해", "This year", "今年", "Este año", "Cette année", "Este ano", "Dieses Jahr", "Этот год"),
  costUnavailable: t("계산 불가", "Unavailable", "計算不可", "No disponible", "Indisponible", "Indisponível", "Nicht verfügbar", "Недоступно"),
  dailyTrend7d: t("최근 7일", "Last 7 days", "直近7日", "Últimos 7 días", "7 derniers jours", "Últimos 7 dias", "Letzte 7 Tage", "За 7 дней"),
  dailyTrend14d: t("최근 14일", "Last 14 days", "直近14日", "Últimos 14 días", "14 derniers jours", "Últimos 14 dias", "Letzte 14 Tage", "За 14 дней"),
  dailyTrend21d: t("최근 21일", "Last 21 days", "直近21日", "Últimos 21 días", "21 derniers jours", "Últimos 21 dias", "Letzte 21 Tage", "За 21 день"),
  dailyTrendMonth: t("이번 달", "This month", "今月", "Este mes", "Ce mois-ci", "Este mês", "Dieser Monat", "Этот месяц"),
  peakDay: t("최다", "Peak", "最多", "Máx.", "Max.", "Máx.", "Max.", "Макс."),
  recentHeatmap: t("최근 16주", "16 weeks", "16週間", "16 semanas", "16 semaines", "16 semanas", "16 Wochen", "16 недель"),
  activitySwitchHint: t("클릭해서 다른 표시로 전환", "Click to switch the view", "クリックで表示を切り替え", "Haz clic para cambiar la vista", "Cliquez pour changer d'affichage", "Clique para alternar a exibição", "Zum Wechseln der Ansicht klicken", "Нажмите, чтобы сменить вид"),
  tokenInput: t("입력", "Input", "入力", "Entrada", "Entrée", "Entrada", "Eingabe", "Ввод"),
  tokenOutput: t("출력", "Output", "出力", "Salida", "Sortie", "Saída", "Ausgabe", "Вывод"),
  tokenCacheWrite: t("캐시 쓰기", "Cache write", "キャッシュ書込", "Escritura caché", "Écriture cache", "Gravação cache", "Cache schreiben", "Запись в кэш"),
  tokenCacheRead: t("캐시 읽기", "Cache read", "キャッシュ読込", "Lectura caché", "Lecture cache", "Leitura cache", "Cache lesen", "Чтение кэша"),
  back: t("뒤로", "Back", "戻る", "Atrás", "Retour", "Voltar", "Zurück", "Назад"),
  recapOpen: t("사용량 돌아보기", "Usage recap", "使用量のふりかえり", "Resumen de uso", "Récap d'utilisation", "Resumo de uso", "Nutzungsrückblick", "Итоги расхода"),
  recapDetails: t("상세", "Details", "詳細", "Detalle", "Détails", "Detalhes", "Details", "Детали"),
  recapTokensUnit: t("토큰", "tokens", "トークン", "tokens", "tokens", "tokens", "Tokens", "токенов"),
  recapWeek: t("주간", "Week", "週", "Semana", "Semaine", "Semana", "Woche", "Неделя"),
  recapMonth: t("월간", "Month", "月", "Mes", "Mois", "Mês", "Monat", "Месяц"),
  recapYear: t("연간", "Year", "年", "Año", "Année", "Ano", "Jahr", "Год"),
  recapCompareWeek: t("지난주 같은 기간과 비교", "Compared with the same days last week", "先週の同じ期間と比較", "Comparado con los mismos días de la semana pasada", "Comparé aux mêmes jours de la semaine dernière", "Comparado com os mesmos dias da semana passada", "Verglichen mit denselben Tagen der Vorwoche", "По сравнению с теми же днями прошлой недели"),
  recapCompareMonth: t("지난달 같은 기간과 비교", "Compared with the same days last month", "先月の同じ期間と比較", "Comparado con los mismos días del mes pasado", "Comparé aux mêmes jours du mois dernier", "Comparado com os mesmos dias do mês passado", "Verglichen mit denselben Tagen des Vormonats", "По сравнению с теми же днями прошлого месяца"),
  recapCompareYear: t("작년 같은 기간과 비교", "Compared with the same days last year", "昨年の同じ期間と比較", "Comparado con los mismos días del año pasado", "Comparé aux mêmes jours de l'an dernier", "Comparado com os mesmos dias do ano passado", "Verglichen mit denselben Tagen des Vorjahres", "По сравнению с теми же днями прошлого года"),
  recapPrevious: t("이전", "Previous", "前へ", "Anterior", "Précédent", "Anterior", "Vorherige", "Назад"),
  recapNext: t("다음", "Next", "次へ", "Siguiente", "Suivant", "Próximo", "Nächste", "Вперёд"),
  recapBestDay: t("최고의 날", "Best day", "最高の日", "Mejor día", "Meilleur jour", "Melhor dia", "Bester Tag", "Лучший день"),
  recapActiveDays: t("활동한 날", "Active days", "稼働日", "Días activos", "Jours actifs", "Dias ativos", "Aktive Tage", "Активных дней"),
  recapBestStreak: t("최장 연속", "Best streak", "最長連続", "Mejor racha", "Meilleure série", "Melhor sequência", "Beste Serie", "Лучшая серия"),
  recapGraduates: t("졸업", "Graduated", "卒業", "Graduados", "Diplômés", "Formados", "Abschlüsse", "Выпускники"),
  recapNoData: t("기록 없음", "no data", "記録なし", "sin datos", "pas de données", "sem dados", "keine Daten", "нет данных"),
  recapNoGraduates: t("이 기간에 졸업한 포켓몬이 없어요.", "No graduation in this period.", "この期間の卒業はありません。", "Ninguna graduación en este periodo.", "Aucun diplômé sur cette période.", "Nenhuma formatura neste período.", "Kein Abschluss in diesem Zeitraum.", "За этот период выпускников нет."),
  streakDays: f<[number]>(
    (n) => `${n}일 연속 사용`, (n) => `${n}-day streak`, (n) => `${n}日連続`, (n) => `Racha de ${n} días`,
    (n) => `Série de ${n} jours`, (n) => `${n} dias seguidos`, (n) => `${n} Tage in Folge`, (n) => `${n} дн. подряд`,
  ),
  streakDaysPending: f<[number]>(
    (n) => `${n}일 연속 — 오늘 사용하면 이어집니다`,
    (n) => `${n}-day streak — use it today to keep it going`,
    (n) => `${n}日連続 — 今日使えば続きます`,
    (n) => `Racha de ${n} días: úsalo hoy para mantenerla`,
    (n) => `Série de ${n} jours — utilisez-le aujourd'hui pour la garder`,
    (n) => `${n} dias seguidos — use hoje para manter`,
    (n) => `${n} Tage in Folge – heute nutzen, um sie zu halten`,
    (n) => `${n} дн. подряд — используйте сегодня, чтобы не прервать`,
  ),
  recapDays: f<[number]>(
    (n) => `${n}일`, (n) => `${n}d`, (n) => `${n}日`, (n) => `${n} d`, (n) => `${n} j`, (n) => `${n} d`, (n) => `${n} T`, (n) => `${n} дн.`,
  ),
  recapBestDayLine: f<[string, string]>(
    (d, n) => `최고의 날은 ${d} — ${n} 토큰.`,
    (d, n) => `Your best day was ${d} — ${n} tokens.`,
    (d, n) => `最高の日は ${d}、${n} トークン。`,
    (d, n) => `Tu mejor día fue el ${d}: ${n} tokens.`,
    (d, n) => `Ton meilleur jour, c'était ${d} : ${n} tokens.`,
    (d, n) => `Seu melhor dia foi ${d}: ${n} tokens.`,
    (d, n) => `Dein bester Tag war ${d} — ${n} Tokens.`,
    (d, n) => `Лучший день — ${d}, токенов: ${n}.`,
  ),
};

type HomeDict = { [K in keyof typeof rows]: (typeof rows)[K][1] };

function transpose(): Strings<HomeDict> {
  const out = {} as Record<string, Record<string, unknown>>;
  LANGS.forEach((lang) => (out[lang] = {}));
  for (const [key, values] of Object.entries(rows)) {
    // Localization.swift argument order: ko, en, ja, es, fr, pt, de, ru (same as LANGS).
    LANGS.forEach((lang, i) => (out[lang][key] = values[i]));
  }
  return out as unknown as Strings<HomeDict>;
}

export const homeStrings = defineStrings(transpose());
