// Plattform der Oberfläche — für Tastenbeschriftungen (⌘ unter macOS, Strg/Ctrl sonst).

const nav: Navigator | undefined = typeof navigator === 'undefined' ? undefined : navigator;
const platform = (nav as (Navigator & { userAgentData?: { platform?: string } }) | undefined)?.userAgentData?.platform || nav?.platform || nav?.userAgent || '';

export const IS_MAC = /mac|iphone|ipad/i.test(platform);

/** Beschriftung der Befehlstaste: macOS ⌘, Windows/Linux „Strg“ (DE) bzw. „Ctrl“ (EN). */
export function modKey(lang: 'de' | 'en'): string {
  if (IS_MAC) return '⌘';
  return lang === 'de' ? 'Strg' : 'Ctrl';
}

/** Kürzel als Text, z. B. „⌘ ,“ bzw. „Strg + ,“. */
export function shortcut(lang: 'de' | 'en', key: string): string {
  return IS_MAC ? `⌘ ${key}` : `${modKey(lang)} + ${key}`;
}
