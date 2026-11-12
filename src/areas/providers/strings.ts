// Providers area strings (macOS Localization.swift:674-738). Owner: providers.
// The additional-accounts hint drops the macOS Keychain clause: Windows has no Keychain prompt.
import { defineStrings } from "../../kit/i18n";

export const providerStrings = defineStrings({
  en: {
    trackedProvidersHint:
      "Turned-off providers are not read. They are excluded from tabs, totals, Pokémon growth, and limits.",
    customScanProviderLabel: "Provider",
    customScanRootsLabel: "Additional scan folders",
    customScanRootsHint:
      "Only for this provider's logs outside the built-in locations. Comma/newline separated, * wildcards. Do not point at another provider's folder.",
    customScanRootsPlaceholder: "~/path/to/sessions",
    customScanRootsMatches: (n: number) => `Scans ${n} extra folder(s) now`,
    aggregationNote: "Token basis: totalTokens (input + output + cache, local date)",
    additionalClaudeAccountsLabel: "Additional Claude accounts",
    additionalClaudeAccountsHint:
      "Other Claude Code logins (CLAUDE_CONFIG_DIR) are detected in ~/.claude-* folders and from an exported CLAUDE_CONFIG_DIR. Add config folders stored elsewhere here, comma/newline separated. Each account gets its own tab in the official limits.",
    additionalClaudeAccountsDetected: (folders: string) => `Detected: ${folders}`,
    additionalClaudeAccountsPlaceholder: "~/.claude-work",
    additionalClaudeAccountsFound: (n: number) => `${n} account folder(s) found`,
  },
  ko: {
    trackedProvidersHint: "끈 프로바이더는 로그를 읽지 않아요. 탭·합계·포켓몬 성장·한도에서 빠져요.",
    customScanProviderLabel: "프로바이더",
    customScanRootsLabel: "추가 스캔 폴더",
    customScanRootsHint:
      "선택한 프로바이더의 로그가 기본 위치 밖에 있을 때만. 콤마·줄바꿈 구분, * 와일드카드. 다른 프로바이더 폴더를 넣지 마세요.",
    customScanRootsMatches: (n: number) => `지금 ${n}개 추가 폴더를 스캔함`,
    aggregationNote: "토큰 집계 기준: totalTokens (input + output + cache, 로컬 날짜)",
    additionalClaudeAccountsLabel: "추가 Claude 계정",
    additionalClaudeAccountsHint:
      "다른 Claude Code 로그인(CLAUDE_CONFIG_DIR)은 ~/.claude-* 폴더와 export 된 CLAUDE_CONFIG_DIR 에서 자동으로 찾습니다. 다른 위치의 설정 폴더만 여기 추가하세요(콤마·줄바꿈 구분). 공식 한도에 계정마다 탭이 생깁니다.",
    additionalClaudeAccountsDetected: (folders: string) => `자동 감지: ${folders}`,
    additionalClaudeAccountsFound: (n: number) => `계정 폴더 ${n}개 찾음`,
  },
  ja: {
    trackedProvidersHint: "オフにしたプロバイダーのログは読みません。タブ・合計・ポケモンの成長・上限から外れます。",
    customScanProviderLabel: "プロバイダー",
    customScanRootsLabel: "追加スキャンフォルダ",
    customScanRootsHint:
      "選択したプロバイダーのログが既定の場所にないときだけ。カンマ・改行区切り、*ワイルドカード。別プロバイダーのフォルダは指定しないでください。",
    customScanRootsMatches: (n: number) => `現在${n}個の追加フォルダをスキャン`,
    aggregationNote: "集計基準: totalTokens (input + output + cache, ローカル日付)",
    additionalClaudeAccountsLabel: "追加のClaudeアカウント",
    additionalClaudeAccountsHint:
      "他のClaude Codeログイン(CLAUDE_CONFIG_DIR)は ~/.claude-* フォルダと、エクスポートされた CLAUDE_CONFIG_DIR から自動検出されます。別の場所にある設定フォルダだけをここに追加してください(カンマ・改行区切り)。公式上限にアカウントごとのタブが追加されます。",
    additionalClaudeAccountsDetected: (folders: string) => `自動検出: ${folders}`,
    additionalClaudeAccountsFound: (n: number) => `アカウントフォルダが${n}個見つかりました`,
  },
  es: {
    trackedProvidersHint:
      "Los proveedores desactivados no se leen. Dejan de aparecer en pestañas, totales, crecimiento del Pokémon y límites.",
    customScanProviderLabel: "Proveedor",
    customScanRootsLabel: "Carpetas de escaneo adicionales",
    customScanRootsHint:
      "Solo para los registros de este proveedor fuera de las ubicaciones integradas. Separados por coma o salto de línea; comodines *. No indiques la carpeta de otro proveedor.",
    customScanRootsMatches: (n: number) => `Escanea ${n} carpeta(s) extra ahora`,
    aggregationNote: "Base de cálculo: totalTokens (input + output + cache, fecha local)",
    additionalClaudeAccountsLabel: "Cuentas de Claude adicionales",
    additionalClaudeAccountsHint:
      "Los otros inicios de sesión de Claude Code (CLAUDE_CONFIG_DIR) se detectan en las carpetas ~/.claude-* y en un CLAUDE_CONFIG_DIR exportado. Añade aquí las carpetas de configuración guardadas en otro lugar, separadas por coma o salto de línea. Cada cuenta tiene su pestaña en los límites oficiales.",
    additionalClaudeAccountsDetected: (folders: string) => `Detectadas: ${folders}`,
    additionalClaudeAccountsFound: (n: number) => `${n} carpeta(s) de cuenta encontrada(s)`,
  },
  fr: {
    trackedProvidersHint:
      "Les fournisseurs désactivés ne sont pas lus. Ils disparaissent des onglets, des totaux, de la croissance du Pokémon et des limites.",
    customScanProviderLabel: "Fournisseur",
    customScanRootsLabel: "Dossiers d'analyse supplémentaires",
    customScanRootsHint:
      "Uniquement pour les journaux de ce fournisseur en dehors des emplacements intégrés. Séparés par des virgules ou des retours à la ligne ; caractères génériques *. N'indique pas le dossier d'un autre fournisseur.",
    customScanRootsMatches: (n: number) => `Analyse ${n} dossier(s) supplémentaire(s) maintenant`,
    aggregationNote: "Base de calcul : totalTokens (input + output + cache, date locale)",
    additionalClaudeAccountsLabel: "Comptes Claude supplémentaires",
    additionalClaudeAccountsHint:
      "Les autres connexions Claude Code (CLAUDE_CONFIG_DIR) sont détectées dans les dossiers ~/.claude-* et depuis un CLAUDE_CONFIG_DIR exporté. Ajoute ici les dossiers de config rangés ailleurs, séparés par des virgules ou des retours à la ligne. Chaque compte a son onglet dans les limites officielles.",
    additionalClaudeAccountsDetected: (folders: string) => `Détectés : ${folders}`,
    additionalClaudeAccountsFound: (n: number) => `${n} dossier(s) de compte trouvé(s)`,
  },
  pt: {
    trackedProvidersHint:
      "Provedores desativados não são lidos. Eles saem das abas, dos totais, do crescimento do Pokémon e dos limites.",
    customScanProviderLabel: "Provedor",
    customScanRootsLabel: "Pastas extras para escanear",
    customScanRootsHint:
      "Só para os logs deste provedor fora dos locais padrão. Separados por vírgula ou quebra de linha; curingas *. Não aponte para a pasta de outro provedor.",
    customScanRootsMatches: (n: number) => `Escaneando ${n} pasta(s) extra agora`,
    aggregationNote: "Base de cálculo: totalTokens (input + output + cache, data local)",
    additionalClaudeAccountsLabel: "Contas extras do Claude",
    additionalClaudeAccountsHint:
      "Os outros logins do Claude Code (CLAUDE_CONFIG_DIR) são detectados nas pastas ~/.claude-* e num CLAUDE_CONFIG_DIR exportado. Adicione aqui pastas de configuração guardadas em outro lugar, separadas por vírgula ou quebra de linha. Cada conta ganha sua aba nos limites oficiais.",
    additionalClaudeAccountsDetected: (folders: string) => `Detectadas: ${folders}`,
    additionalClaudeAccountsFound: (n: number) => `${n} pasta(s) de conta encontrada(s)`,
  },
  de: {
    trackedProvidersHint:
      "Deaktivierte Anbieter werden nicht gelesen. Sie fehlen in Tabs, Summen, Pokémon-Wachstum und Limits.",
    customScanProviderLabel: "Anbieter",
    customScanRootsLabel: "Zusätzliche Scan-Ordner",
    customScanRootsHint:
      "Nur für Protokolle dieses Anbieters außerhalb der Standardpfade. Durch Kommas oder Zeilenumbrüche getrennt, * als Platzhalter. Wähle keinen Ordner eines anderen Anbieters.",
    customScanRootsMatches: (n: number) => `Zusätzlich gescannte Ordner: ${n}`,
    aggregationNote: "Token-Grundlage: totalTokens (input + output + cache, lokales Datum)",
    additionalClaudeAccountsLabel: "Weitere Claude-Konten",
    additionalClaudeAccountsHint:
      "Weitere Claude-Code-Anmeldungen (CLAUDE_CONFIG_DIR) werden in ~/.claude-*-Ordnern und über ein exportiertes CLAUDE_CONFIG_DIR erkannt. Füge hier Konfigurationsordner an anderen Orten hinzu, durch Kommas oder Zeilenumbrüche getrennt. Jedes Konto bekommt bei den offiziellen Limits einen eigenen Tab.",
    additionalClaudeAccountsDetected: (folders: string) => `Erkannt: ${folders}`,
    additionalClaudeAccountsFound: (n: number) => `Gefundene Kontoordner: ${n}`,
  },
  ru: {
    trackedProvidersHint:
      "Отключённые провайдеры не читаются. Они пропадают из вкладок, итогов, роста покемона и лимитов.",
    customScanProviderLabel: "Провайдер",
    customScanRootsLabel: "Дополнительные папки для сканирования",
    customScanRootsHint:
      "Только для логов этого провайдера вне стандартных мест. Через запятую или с новой строки, поддерживается подстановка *. Не указывайте папку другого провайдера.",
    customScanRootsMatches: (n: number) => `Сканируется дополнительных папок: ${n}`,
    aggregationNote: "Подсчёт токенов: totalTokens (ввод + вывод + кэш, по местной дате)",
    additionalClaudeAccountsLabel: "Дополнительные аккаунты Claude",
    additionalClaudeAccountsHint:
      "Другие входы в Claude Code (CLAUDE_CONFIG_DIR) находятся автоматически в папках ~/.claude-* и по экспортированной переменной CLAUDE_CONFIG_DIR. Папки конфигурации в других местах добавьте здесь — через запятую или с новой строки. У каждого аккаунта своя вкладка в официальных лимитах.",
    additionalClaudeAccountsDetected: (folders: string) => `Найдено: ${folders}`,
    additionalClaudeAccountsFound: (n: number) => `Найдено папок аккаунтов: ${n}`,
  },
});
