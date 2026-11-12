export const LANG_NAMES: Record<string, string> = { en: "English", fr: "French", ar: "Arabic" };

export function langName(code: string | null | undefined): string {
  if (!code) return "Unknown";
  return LANG_NAMES[code] ?? code.toUpperCase();
}

export function graphemes(text: string): string[] {
  const seg = new Intl.Segmenter(undefined, { granularity: "grapheme" });
  return Array.from(seg.segment(text), (s) => s.segment);
}
