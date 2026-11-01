// A safe markdown subset for the session file's exchange (story 4.20, the
// owner's revision of D-706, D-1045). The prompt and the answer are
// untrusted session text: this module only reads them into a tree of
// plain strings, and `Markdown.svelte` writes each string as a text node.
// Nothing is ever parsed as HTML, so a tag in the text shows as written;
// a link shows its text and its address, never as an anchor; an image
// shows its alt text and is never loaded. Every structure is bounded (the
// source's length, a quote's depth, an inline's nesting, a table's size),
// and a marker that does not close is text.

export type Inline =
  | { kind: "text"; text: string }
  | { kind: "break" }
  | { kind: "code"; text: string }
  | { kind: "strong"; children: Inline[] }
  | { kind: "em"; children: Inline[] }
  | { kind: "link"; children: Inline[]; url: string }
  | { kind: "image"; alt: string };

export type Align = "left" | "center" | "right" | null;

export interface ListItem {
  inlines: Inline[];
  /** Its indentation level, 0 for the outermost. */
  depth: number;
}

export type Block =
  | { kind: "paragraph"; inlines: Inline[] }
  | { kind: "heading"; inlines: Inline[] }
  | { kind: "code"; text: string; info: string }
  | { kind: "list"; ordered: boolean; start: number; items: ListItem[] }
  | { kind: "quote"; blocks: Block[] }
  | { kind: "table"; align: Align[]; header: Inline[][]; rows: Inline[][][] }
  | { kind: "rule" };

/** The bounds every input is held to. */
export const MARKDOWN_LIMITS = {
  /** Past this many characters the rest is one plain paragraph. */
  chars: 8_000,
  /** A quote inside quotes deeper than this is read as text. */
  quoteDepth: 4,
  /** Emphasis inside emphasis deeper than this is read as text. */
  inlineDepth: 6,
  /** A list item's indentation level at most. */
  listDepth: 3,
  /** How far a link's text runs before it is no link. */
  linkText: 1_000,
  /** A table's columns and body rows at most; the rest is left out. */
  tableColumns: 24,
  tableRows: 100,
} as const;

const FENCE = /^ {0,3}(`{3,}|~{3,})(.*)$/;
const HEADING = /^ {0,3}(#{1,6})(?:[ \t]+(.*?))?(?:[ \t]+#+)?[ \t]*$/;
const RULE = /^ {0,3}([-*_])(?:[ \t]*\1){2,}[ \t]*$/;
const QUOTE = /^ {0,3}> ?(.*)$/;
const ITEM = /^( *)([-*+]|\d{1,9}[.)])[ \t]+(.*)$/;
const DELIMITER_ROW = /^ *\|? *:?-+:? *(\| *:?-+:? *)*\|? *$/;
const PUNCTUATION = /[!-/:-@[-`{-~]/;

/** Reads `source` into blocks of plain strings. Never throws. */
export function parseMarkdown(source: string): Block[] {
  const text = source.replace(/\r\n?/g, "\n");
  const head = text.slice(0, MARKDOWN_LIMITS.chars);
  const blocks = parseBlocks(head.split("\n"), 0);
  const rest = text.slice(MARKDOWN_LIMITS.chars);
  if (rest.length > 0) blocks.push({ kind: "paragraph", inlines: [{ kind: "text", text: rest }] });
  return blocks;
}

/** A fence that opens a code block: a backtick fence's info string holds
 *  no backtick (CommonMark), else the line is text. */
function openingFence(line: string): RegExpExecArray | null {
  const fence = FENCE.exec(line);
  return fence && !(fence[1]![0] === "`" && fence[2]!.includes("`")) ? fence : null;
}

function startsBlock(line: string): boolean {
  return openingFence(line) !== null || HEADING.test(line) || RULE.test(line) || QUOTE.test(line) || ITEM.test(line);
}

function parseBlocks(lines: string[], quoteDepth: number): Block[] {
  const blocks: Block[] = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i]!;
    if (line.trim() === "") {
      i++;
      continue;
    }
    const fence = openingFence(line);
    if (fence) {
      const marker = fence[1]!;
      const body: string[] = [];
      i++;
      while (i < lines.length) {
        const close = /^ {0,3}(`{3,}|~{3,})[ \t]*$/.exec(lines[i]!);
        if (close && close[1]![0] === marker[0] && close[1]!.length >= marker.length) {
          i++;
          break;
        }
        body.push(lines[i]!);
        i++;
      }
      blocks.push({ kind: "code", text: body.join("\n"), info: fence[2]!.trim() });
      continue;
    }
    const heading = HEADING.exec(line);
    if (heading) {
      blocks.push({ kind: "heading", inlines: parseInline(heading[2] ?? "", 0) });
      i++;
      continue;
    }
    if (RULE.test(line)) {
      blocks.push({ kind: "rule" });
      i++;
      continue;
    }
    if (QUOTE.test(line)) {
      const inner: string[] = [];
      while (i < lines.length && QUOTE.test(lines[i]!)) {
        inner.push(QUOTE.exec(lines[i]!)![1]!);
        i++;
      }
      if (quoteDepth + 1 > MARKDOWN_LIMITS.quoteDepth) {
        blocks.push({ kind: "paragraph", inlines: joinLines(inner) });
      } else {
        blocks.push({ kind: "quote", blocks: parseBlocks(inner, quoteDepth + 1) });
      }
      continue;
    }
    const item = ITEM.exec(line);
    if (item) {
      const ordered = /\d/.test(item[2]![0]!);
      const list: Extract<Block, { kind: "list" }> = { kind: "list", ordered, start: ordered ? Number.parseInt(item[2]!, 10) : 1, items: [] };
      const base = item[1]!.length;
      while (i < lines.length) {
        const next = ITEM.exec(lines[i]!);
        if (next) {
          if (/\d/.test(next[2]![0]!) !== ordered) break;
          const depth = Math.min(MARKDOWN_LIMITS.listDepth, Math.floor(Math.max(0, next[1]!.length - base) / 2));
          list.items.push({ inlines: parseInline(next[3]!, 0), depth });
          i++;
          continue;
        }
        // An indented line carries on the item above it.
        const carried = /^ {2,}(\S.*)$/.exec(lines[i]!);
        const last = list.items[list.items.length - 1];
        if (carried && last) {
          last.inlines.push({ kind: "break" }, ...parseInline(carried[1]!, 0));
          i++;
          continue;
        }
        break;
      }
      blocks.push(list);
      continue;
    }
    if (line.includes("|") && i + 1 < lines.length && DELIMITER_ROW.test(lines[i + 1]!) && lines[i + 1]!.includes("-")) {
      const header = cells(line).slice(0, MARKDOWN_LIMITS.tableColumns);
      const align = cells(lines[i + 1]!)
        .slice(0, header.length)
        .map(alignOf);
      while (align.length < header.length) align.push(null);
      const rows: Inline[][][] = [];
      i += 2;
      while (i < lines.length && lines[i]!.includes("|") && lines[i]!.trim() !== "") {
        if (rows.length < MARKDOWN_LIMITS.tableRows) {
          const row = cells(lines[i]!).slice(0, header.length);
          while (row.length < header.length) row.push("");
          rows.push(row.map((cell) => parseInline(cell, 0)));
        }
        i++;
      }
      blocks.push({ kind: "table", align, header: header.map((cell) => parseInline(cell, 0)), rows });
      continue;
    }
    const paragraph: string[] = [];
    while (i < lines.length && lines[i]!.trim() !== "" && (paragraph.length === 0 || !startsBlock(lines[i]!))) {
      paragraph.push(lines[i]!);
      i++;
    }
    blocks.push({ kind: "paragraph", inlines: joinLines(paragraph) });
  }
  return blocks;
}

/** A paragraph's lines, each break kept: a message's line breaks are its
 *  author's. */
function joinLines(lines: string[]): Inline[] {
  const out: Inline[] = [];
  lines.forEach((line, n) => {
    if (n > 0) out.push({ kind: "break" });
    out.push(...parseInline(line.trim(), 0));
  });
  return out;
}

/** A table row's cells: split on the pipes no backslash escapes, the
 *  outer pipes dropped. */
function cells(row: string): string[] {
  let text = row.trim();
  if (text.startsWith("|")) text = text.slice(1);
  if (text.endsWith("|") && !text.endsWith("\\|")) text = text.slice(0, -1);
  const out: string[] = [];
  let cell = "";
  for (let i = 0; i < text.length; i++) {
    const c = text[i]!;
    if (c === "\\" && text[i + 1] === "|") {
      cell += "|";
      i++;
    } else if (c === "|") {
      out.push(cell.trim());
      cell = "";
    } else {
      cell += c;
    }
  }
  out.push(cell.trim());
  return out;
}

function alignOf(cell: string): Align {
  const left = cell.startsWith(":");
  const right = cell.endsWith(":");
  return left && right ? "center" : right ? "right" : left ? "left" : null;
}

function pushText(out: Inline[], text: string): void {
  if (text === "") return;
  const last = out[out.length - 1];
  if (last?.kind === "text") last.text += text;
  else out.push({ kind: "text", text });
}

const isWordChar = (c: string | undefined): boolean => c !== undefined && /[\p{L}\p{N}]/u.test(c);

/** The index where an emphasis run `marker` opened at `from` closes, or -1:
 *  the closing run follows a non-space, and an underscore run is not
 *  inside a word. */
function closing(s: string, marker: string, from: number): number {
  let at = s.indexOf(marker, from);
  while (at !== -1) {
    const before = s[at - 1];
    const after = s[at + marker.length];
    const flanked = before !== undefined && !/\s/.test(before);
    const intraword = marker[0] === "_" && isWordChar(after);
    // A longer run of the same character is not this marker's end.
    const longer = after === marker[0];
    if (at > from && flanked && !intraword && !longer) return at;
    at = s.indexOf(marker, at + 1);
  }
  return -1;
}

/** `[text](url)` from `start`, the opening bracket: its text and address
 *  and where it ends, or `null` when it is not one. */
function link(s: string, start: number): { text: string; url: string; end: number } | null {
  let depth = 0;
  let close = -1;
  const last = Math.min(s.length, start + MARKDOWN_LIMITS.linkText);
  for (let i = start; i < last; i++) {
    const c = s[i];
    if (c === "\\") {
      i++;
      continue;
    }
    if (c === "[") depth++;
    else if (c === "]" && --depth === 0) {
      close = i;
      break;
    }
  }
  if (close === -1 || s[close + 1] !== "(") return null;
  const end = s.indexOf(")", close + 2);
  if (end === -1) return null;
  const target = s.slice(close + 2, end).trim();
  const url = target.split(/\s+/)[0] ?? "";
  if (url === "") return null;
  return { text: s.slice(start + 1, close), url: url.replace(/^<(.*)>$/, "$1"), end: end + 1 };
}

/** A line's inlines: code spans first, then images and links, then strong
 *  and emphasis; anything that does not close is text. */
export function parseInline(s: string, depth: number): Inline[] {
  const out: Inline[] = [];
  if (depth > MARKDOWN_LIMITS.inlineDepth) {
    pushText(out, s);
    return out;
  }
  let i = 0;
  while (i < s.length) {
    const c = s[i]!;
    if (c === "\\" && i + 1 < s.length && PUNCTUATION.test(s[i + 1]!)) {
      pushText(out, s[i + 1]!);
      i += 2;
      continue;
    }
    if (c === "`") {
      let n = 1;
      while (s[i + n] === "`") n++;
      const fence = "`".repeat(n);
      let end = s.indexOf(fence, i + n);
      while (end !== -1 && s[end + n] === "`") end = s.indexOf(fence, end + n + 1);
      if (end === -1) {
        pushText(out, fence);
        i += n;
        continue;
      }
      let code = s.slice(i + n, end);
      if (code.length > 2 && code.startsWith(" ") && code.endsWith(" ")) code = code.slice(1, -1);
      out.push({ kind: "code", text: code });
      i = end + n;
      continue;
    }
    if (c === "!" && s[i + 1] === "[") {
      const found = link(s, i + 1);
      if (found) {
        out.push({ kind: "image", alt: found.text });
        i = found.end;
        continue;
      }
    }
    if (c === "[") {
      const found = link(s, i);
      if (found) {
        out.push({ kind: "link", children: parseInline(found.text, depth + 1), url: found.url });
        i = found.end;
        continue;
      }
    }
    if (c === "*" || c === "_") {
      const double = s[i + 1] === c;
      const marker = double ? c + c : c;
      const opens = s[i + marker.length] !== undefined && !/\s/.test(s[i + marker.length]!) && !(c === "_" && isWordChar(s[i - 1]));
      if (opens) {
        const end = closing(s, marker, i + marker.length);
        if (end !== -1) {
          const children = parseInline(s.slice(i + marker.length, end), depth + 1);
          out.push(double ? { kind: "strong", children } : { kind: "em", children });
          i = end + marker.length;
          continue;
        }
      }
      pushText(out, marker);
      i += marker.length;
      continue;
    }
    let j = i + 1;
    while (j < s.length && !"\\`![*_".includes(s[j]!)) j++;
    pushText(out, s.slice(i, j));
    i = j;
  }
  return out;
}
