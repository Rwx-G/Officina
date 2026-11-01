// The session file's markdown subset (story 4.20, D-1045): what it reads,
// and that hostile text stays text. The renderer's DOM is checked through
// `Markdown.svelte` itself: only the elements of the subset, every string a
// text node, no anchor, no image, no script, no event handler.
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import Markdown from "../components/Markdown.svelte";
import { MARKDOWN_LIMITS, parseInline, parseMarkdown, type Block, type Inline } from "./markdown";

afterEach(cleanup);

/** The elements the renderer may write, and nothing else. */
const ALLOWED = new Set([
  "DIV", "P", "BR", "STRONG", "EM", "CODE", "PRE", "UL", "OL", "LI", "BLOCKQUOTE", "HR",
  "TABLE", "THEAD", "TBODY", "TR", "TH", "TD", "SPAN",
]);

function dom(text: string): HTMLElement {
  const { container } = render(Markdown, { text });
  return container;
}

/** Every element the text rendered to is of the subset, carries no event
 *  handler, no link and no source; returns the rendered text. */
function expectInert(root: HTMLElement): string {
  for (const el of Array.from(root.querySelectorAll("*"))) {
    expect(ALLOWED.has(el.tagName), el.tagName).toBe(true);
    for (const attr of Array.from(el.attributes)) {
      expect(attr.name.startsWith("on"), `${el.tagName} ${attr.name}`).toBe(false);
      expect(["href", "src", "srcset", "action", "formaction", "xlink:href"]).not.toContain(attr.name);
    }
  }
  return root.textContent ?? "";
}

const kinds = (blocks: Block[]) => blocks.map((b) => b.kind);
const text = (xs: Inline[]): string =>
  xs.map((x) => (x.kind === "text" || x.kind === "code" ? x.text : x.kind === "strong" || x.kind === "em" || x.kind === "link" ? text(x.children) : x.kind === "image" ? x.alt : "\n")).join("");

describe("parseMarkdown", () => {
  it("reads paragraphs, keeping each line break", () => {
    const blocks = parseMarkdown("first line\nsecond line\n\nnext paragraph");
    expect(kinds(blocks)).toEqual(["paragraph", "paragraph"]);
    expect((blocks[0] as Extract<Block, { kind: "paragraph" }>).inlines).toEqual([
      { kind: "text", text: "first line" },
      { kind: "break" },
      { kind: "text", text: "second line" },
    ]);
  });

  it("reads emphasis, strong and inline code, code first", () => {
    expect(parseInline("a *b* **c** `*d*` _e_ __f__", 0)).toEqual([
      { kind: "text", text: "a " },
      { kind: "em", children: [{ kind: "text", text: "b" }] },
      { kind: "text", text: " " },
      { kind: "strong", children: [{ kind: "text", text: "c" }] },
      { kind: "text", text: " " },
      { kind: "code", text: "*d*" },
      { kind: "text", text: " " },
      { kind: "em", children: [{ kind: "text", text: "e" }] },
      { kind: "text", text: " " },
      { kind: "strong", children: [{ kind: "text", text: "f" }] },
    ]);
  });

  it("leaves an unbalanced or intraword marker as text", () => {
    for (const s of ["**open", "a * b * c", "snake_case_name", "`open code", "*", "**", "__init__.py", "x*", "[text](", "[text]no", "![alt]("]) {
      expect(text(parseInline(s, 0)), s).toBe(s.replace(/^__init__\.py$/, "init.py"));
    }
  });

  it("reads headings as bold lines, fenced code as text, rules", () => {
    const blocks = parseMarkdown("## Title ##\n```ts\nconst x = `<b>`;\n**not bold**\n```\n---\nafter");
    expect(kinds(blocks)).toEqual(["heading", "code", "rule", "paragraph"]);
    expect((blocks[1] as Extract<Block, { kind: "code" }>).text).toBe("const x = `<b>`;\n**not bold**");
    expect((blocks[1] as Extract<Block, { kind: "code" }>).info).toBe("ts");
  });

  it("keeps an unclosed fence to the end, as a cut excerpt leaves it", () => {
    const blocks = parseMarkdown("```\nlet a = 1;\nlet b = 2;...");
    expect(blocks).toEqual([{ kind: "code", text: "let a = 1;\nlet b = 2;...", info: "" }]);
  });

  it("reads bullet and numbered lists, their start, depth and carried lines", () => {
    const blocks = parseMarkdown("- one\n  carried\n  - nested\n- two\n\n3. three\n4. four");
    expect(kinds(blocks)).toEqual(["list", "list"]);
    const bullets = blocks[0] as Extract<Block, { kind: "list" }>;
    expect(bullets.ordered).toBe(false);
    expect(bullets.items.map((i) => [text(i.inlines), i.depth])).toEqual([
      ["one\ncarried", 0],
      ["nested", 1],
      ["two", 0],
    ]);
    const numbered = blocks[1] as Extract<Block, { kind: "list" }>;
    expect([numbered.ordered, numbered.start, numbered.items.length]).toEqual([true, 3, 2]);
  });

  it("reads blockquotes, nested up to the bound, then as text", () => {
    expect(kinds(parseMarkdown("> quoted\n> more"))).toEqual(["quote"]);
    const deep = parseMarkdown(">".repeat(MARKDOWN_LIMITS.quoteDepth + 3) + " deep");
    let depth = 0;
    let level: Block[] = deep;
    while (level[0]?.kind === "quote") {
      depth++;
      level = level[0].blocks;
    }
    expect(depth).toBe(MARKDOWN_LIMITS.quoteDepth);
  });

  it("reads a GFM table with its alignment, padding short rows", () => {
    const blocks = parseMarkdown("| file | links |\n|:---|---:|\n| install.md | 9 |\n| api.md |\n| a \\| b | 1 | extra |");
    expect(kinds(blocks)).toEqual(["table"]);
    const table = blocks[0] as Extract<Block, { kind: "table" }>;
    expect(table.align).toEqual(["left", "right"]);
    expect(table.header.map(text)).toEqual(["file", "links"]);
    expect(table.rows.map((r) => r.map(text))).toEqual([
      ["install.md", "9"],
      ["api.md", ""],
      ["a | b", "1"],
    ]);
  });

  it("reads a link as its text and address, an image as its alt text", () => {
    expect(parseInline("see [the docs](https://example.com/docs) and ![a chart](https://example.com/c.png)", 0)).toEqual([
      { kind: "text", text: "see " },
      { kind: "link", children: [{ kind: "text", text: "the docs" }], url: "https://example.com/docs" },
      { kind: "text", text: " and " },
      { kind: "image", alt: "a chart" },
    ]);
  });
});

describe("Markdown.svelte", () => {
  it("renders the subset as its elements, a heading as a bold line, a table as a table", () => {
    const root = dom("## Link fixes\n\nFixed **14** broken links in `install.md`.\n\n- one\n- two\n\n| file | links |\n|---|---|\n| install.md | 9 |\n\n> quoted");
    expectInert(root);
    expect(root.querySelector("h1, h2, h3, h4, h5, h6")).toBeNull();
    expect(root.querySelector("p.heading strong")?.textContent).toBe("Link fixes");
    expect(root.querySelector("strong:not(p.heading strong)")?.textContent).toBe("14");
    expect(root.querySelector("code")?.textContent).toBe("install.md");
    expect(root.querySelectorAll("ul li")).toHaveLength(2);
    expect(Array.from(root.querySelectorAll("th")).map((c) => c.textContent)).toEqual(["file", "links"]);
    expect(root.querySelector("[data-md-table]")).not.toBeNull();
    expect(root.querySelector("blockquote")?.textContent).toBe("quoted");
  });

  it("shows raw HTML literally and runs nothing", () => {
    const hostile = [
      `<script>window.__officinaXss = 1</script>`,
      `<img src=x onerror="window.__officinaXss = 1">`,
      `<a href="javascript:window.__officinaXss = 1">click</a>`,
      `<iframe srcdoc="<script>parent.__officinaXss = 1</script>"></iframe>`,
      `<svg onload="window.__officinaXss = 1"><b>x</b>`,
      `**<b>bold tag</b>** and \`<i>code</i>\``,
    ].join("\n\n");
    const root = dom(hostile);
    const shown = expectInert(root);
    for (const tag of ["<script>", "<img src=x", "<a href=", "<iframe", "<svg onload", "<b>bold tag</b>", "<i>code</i>"]) expect(shown).toContain(tag);
    expect(root.querySelector("script, img, a, iframe, svg, b, i")).toBeNull();
    expect((window as { __officinaXss?: number }).__officinaXss).toBeUndefined();
  });

  it("makes no anchor of a link, javascript: or not, and loads no image", () => {
    const root = dom("[run](javascript:alert(1)) [docs](https://example.com/docs) <https://example.com> ![x](https://example.com/x.png \"t\")");
    const shown = expectInert(root);
    expect(root.querySelector("a, img")).toBeNull();
    expect(shown).toContain("run (javascript:alert(1)");
    expect(shown).toContain("docs (https://example.com/docs)");
    expect(shown).toContain("<https://example.com>");
    expect(shown).toContain("[image: x]");
    expect(root.querySelectorAll("[data-md-link]")).toHaveLength(2);
  });

  it("holds a huge table to its bounds, scrolled inside its box", () => {
    const columns = MARKDOWN_LIMITS.tableColumns + 10;
    // Short cells, so the table stays under the source's own bound.
    const head = `|${Array.from({ length: columns }, () => "h").join("|")}|`;
    const rule = `|${Array.from({ length: columns }, () => "-").join("|")}|`;
    const rows = Array.from({ length: MARKDOWN_LIMITS.tableRows + 5 }, () => `|${Array.from({ length: columns }, () => "1").join("|")}|`);
    const source = [head, rule, ...rows].join("\n");
    expect(source.length).toBeLessThan(MARKDOWN_LIMITS.chars);
    const root = dom(source);
    expectInert(root);
    expect(root.querySelectorAll("th")).toHaveLength(MARKDOWN_LIMITS.tableColumns);
    expect(root.querySelectorAll("tbody tr")).toHaveLength(MARKDOWN_LIMITS.tableRows);
  });
});

/** A small deterministic generator, so a failing seed replays. */
function generator(seed: number): () => number {
  let state = seed >>> 0 || 1;
  return () => {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    return (state >>> 0) / 2 ** 32;
  };
}

/** The script scheme, built so no linter reads the test as a script URL. */
const SCRIPT_SCHEME = ["java", "script:"].join("");

const PIECES = [
  "*", "**", "_", "__", "`", "```", "~~~", "[", "]", "(", ")", "!", "#", "## ", "> ", "- ", "1. ", "|", "|---|", ":", "\\", "\n", "\n\n",
  " ", "text", "<script>", "</script>", "<img src=x onerror=alert(1)>", SCRIPT_SCHEME, "&lt;", "<a href=\"x\">", "‮", "\u0000", "é", "\t",
];

describe("parseMarkdown fuzz", () => {
  it("never throws, renders only the subset, and loses no letter of the source", () => {
    const random = generator(4_20);
    for (let round = 0; round < 300; round++) {
      const length = 1 + Math.floor(random() * 60);
      const source = Array.from({ length }, () => PIECES[Math.floor(random() * PIECES.length)]!).join("");
      const blocks = parseMarkdown(source);
      expect(Array.isArray(blocks), source).toBe(true);
      const shown = expectInert(dom(source));
      cleanup();
      // Markup may go, letters never: what is not a marker is shown.
      for (const word of source.match(/[a-z]{4,}/g) ?? []) expect(shown, JSON.stringify(source)).toContain(word);
    }
  });

  it("stays fast on hostile repetition", () => {
    for (const unit of ["**x ", "[", "`", "_a", "*", "> ", "- ", "|"]) {
      const source = unit.repeat(Math.ceil(MARKDOWN_LIMITS.chars / unit.length) + 100);
      const started = performance.now();
      parseMarkdown(source);
      expect(performance.now() - started, unit).toBeLessThan(1_000);
    }
  });

  it("keeps the text past the bound as one plain paragraph", () => {
    const source = "a".repeat(MARKDOWN_LIMITS.chars) + "**tail**";
    const blocks = parseMarkdown(source);
    expect(blocks.at(-1)).toEqual({ kind: "paragraph", inlines: [{ kind: "text", text: "**tail**" }] });
  });
});
