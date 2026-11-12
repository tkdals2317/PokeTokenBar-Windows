// Language plumbing. Owner: foundation.
// Each area keeps its own dictionary (`src/areas/<area>/strings.ts`) built with `defineStrings`
// and reads it with `useStrings`; missing keys fall back to English. There is no shared dictionary.
import { createContext, useContext, useMemo } from "react";

export const LANGS = ["ko", "en", "ja", "es", "fr", "pt", "de", "ru"] as const;
export type Lang = (typeof LANGS)[number];
export const LANG_LABELS: Record<Lang, string> = {
  ko: "한국어", en: "English", ja: "日本語", es: "Español", fr: "Français", pt: "Português", de: "Deutsch", ru: "Русский",
};

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type Dict = Record<string, string | ((...args: any[]) => string)>;
export type Strings<D extends Dict> = { en: D } & { [L in Exclude<Lang, "en">]?: Partial<D> };

/** `defineStrings({ en: {...}, ko: {...}, ... })`: English is the full set and the fallback. */
export const defineStrings = <D extends Dict>(s: Strings<D>) => s;

export const pick = <D extends Dict>(s: Strings<D>, lang: Lang): D =>
  lang === "en" ? s.en : ({ ...s.en, ...s[lang] } as D);

export const LangContext = createContext<Lang>("en");
export const useLang = () => useContext(LangContext);

/** Area dictionary in the app language. */
export function useStrings<D extends Dict>(s: Strings<D>): D {
  const lang = useLang();
  return useMemo(() => pick(s, lang), [s, lang]);
}
