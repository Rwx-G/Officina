// Worker text that cannot reorder itself (D-245, review F26): every
// character of `params::FORBIDDEN_FORMAT_RANGES` renders as a visible
// escape such as `<U+202E>`. The ranges come from the vector file the core
// test rebuilds from the constant, so the window cannot drift from it.
import vector from "../../../tests/vectors/format_ranges.json";

/** Each range as its first and last code point, inclusive. */
export const FORMAT_RANGES: readonly (readonly [number, number])[] = vector.ranges.map(
  ([first, last]) => [first ?? 0, last ?? 0] as const,
);

function isFormat(codePoint: number): boolean {
  return FORMAT_RANGES.some(([first, last]) => codePoint >= first && codePoint <= last);
}

/** The escape a format character renders as. */
export function escapeOf(codePoint: number): string {
  return `<U+${codePoint.toString(16).toUpperCase().padStart(4, "0")}>`;
}

/** `text` with each format character replaced by its escape; any other
 *  character is kept as it is. */
export function escapeFormat(text: string): string {
  let out = "";
  for (const char of text) {
    const codePoint = char.codePointAt(0) ?? 0;
    out += isFormat(codePoint) ? escapeOf(codePoint) : char;
  }
  return out;
}
