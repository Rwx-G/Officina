// The session file's polish (story 4.19): the owner found the panel cut off
// abruptly against the header, and the exchange hard to read. The panel is
// a surface standing in the sessions area, a gap under the header and its
// right edge on the header's last control, its shadow whole (D-702); the
// exchange reads as a conversation, each message on its author's side and
// labelled by its author, the text kept as written (D-705, D-706).
import { expect, test, type Page } from "@playwright/test";
import type { Board } from "../src/lib/generated/Board";
import type { Card } from "../src/lib/generated/Card";
import { loadBoard, openScene, transitionsDone } from "./fixtures";

/** The owner's screens (D-543): the file lies over the scene on a
 *  maximised Full HD window and docks beside it on the ultrawide. */
const SCREENS = [
  { width: 1920, height: 1032, docked: false },
  { width: 3440, height: 1392, docked: true },
] as const;
const THEMES = ["light", "dark"] as const;

const cards = (board: Board): Card[] => Object.values(board.columns).flat();
const file = (page: Page) => page.getByTestId("session-file");
const exchange = (page: Page) => file(page).getByRole("list", { name: "Last exchange" });

async function openFile(page: Page, card: Card): Promise<void> {
  await page.getByTestId("scene").locator(`[data-layer="targets"] [data-key=${JSON.stringify(card.key)}]`).click();
  await expect(file(page)).toBeVisible();
  // The slide-in starts on the page's next frame, which the installed
  // clock drives.
  await page.clock.runFor(1_000);
  await transitionsDone(page);
}

/** How far a computed `box-shadow` reaches past each edge of its box. */
function shadowReach(shadow: string): { top: number; right: number; bottom: number; left: number } {
  const lengths = [...shadow.replace(/rgba?\([^)]*\)/, "").matchAll(/(-?[\d.]+)px/g)].map((m) => Number(m[1]));
  const [x = 0, y = 0, blur = 0, spread = 0] = lengths;
  const reach = blur + spread;
  return { top: Math.max(0, reach - y), right: Math.max(0, reach + x), bottom: Math.max(0, reach + y), left: Math.max(0, reach - x) };
}

/** A card of scene-c by its session name. */
function named(board: Board, name: string): Card {
  const card = cards(board).find((c) => c.name === name);
  if (!card) throw new Error(`no card ${name}`);
  return card;
}

/** scene-c's session whose last round went through the real hook with
 *  markup in its prompt and a long markdown answer (story 4.19). */
const MARKUP_SESSION = "link-fixes";

for (const { width, height, docked } of SCREENS) {
  for (const theme of THEMES) {
    test(`the session file stands under the header with whole edges, ${docked ? "docked" : "over the scene"}, ${theme}`, async ({ page }) => {
      await page.emulateMedia({ colorScheme: theme });
      await page.setViewportSize({ width, height });
      const board = loadBoard("scene-c");
      await openScene(page, board);
      await openFile(page, named(board, MARKUP_SESSION));

      const geometry = await page.evaluate(() => {
        const box = (el: Element) => el.getBoundingClientRect();
        const header = document.querySelector("header.bar")!;
        const controls = Array.from(header.querySelectorAll("button, [role=status], .gauges > *")).map(box).filter((r) => r.width > 0);
        const panel = document.querySelector("[data-testid=session-file]")!;
        const style = getComputedStyle(panel);
        const root = getComputedStyle(document.documentElement);
        return {
          header: box(header),
          controlsRight: Math.max(...controls.map((r) => r.right)),
          headerContentBottom: box(header).bottom - parseFloat(getComputedStyle(header).paddingBottom),
          area: box(document.querySelector("[data-testid=sessions-area]")!),
          scene: box(document.querySelector("[data-testid=scene]")!),
          panel: box(panel),
          register: box(document.querySelector("[data-testid=register]")!),
          shadow: style.boxShadow,
          borders: [style.borderTopWidth, style.borderRightWidth, style.borderBottomWidth, style.borderLeftWidth].map(parseFloat),
          borderColours: [style.borderTopColor, style.borderRightColor, style.borderBottomColor, style.borderLeftColor],
          radii: [style.borderTopLeftRadius, style.borderTopRightRadius, style.borderBottomRightRadius, style.borderBottomLeftRadius].map(parseFloat),
          radiusSurface: parseFloat(root.getPropertyValue("--radius-surface")),
        };
      });
      const { header, area, panel } = geometry;
      const reach = shadowReach(geometry.shadow);

      // Under the header, never over it, and a deliberate gap: the panel's
      // shadow lies whole inside the sessions area, which clips it.
      expect(panel.top).toBeGreaterThanOrEqual(header.bottom);
      expect(panel.top - area.top).toBeGreaterThanOrEqual(reach.top);
      expect(panel.top - area.top).toBeGreaterThan(0);
      expect(area.right - panel.right).toBeGreaterThanOrEqual(reach.right);
      expect(area.bottom - panel.bottom).toBeGreaterThanOrEqual(reach.bottom);
      // Aligned on the header: its right edge on the last control's, and as
      // far below the header's content as it stands from the area's right
      // edge.
      expect(Math.abs(panel.right - geometry.controlsRight)).toBeLessThanOrEqual(1);
      expect(Math.abs(panel.top - geometry.headerContentBottom - (area.right - panel.right))).toBeLessThanOrEqual(1);
      // Four drawn edges, rounded as every surface is (D-539).
      for (const width of geometry.borders) expect(width).toBeGreaterThanOrEqual(1);
      expect(new Set(geometry.borderColours).size).toBe(1);
      for (const radius of geometry.radii) expect(radius).toBe(geometry.radiusSurface);
      // Its placement (D-543) holds with the insets. Over the scene it
      // covers the register whole (D-555), no strip of it left showing
      // above (D-704); docked, the register stays in view.
      expect(geometry.scene.right <= panel.left + 0.5).toBe(docked);
      const { register } = geometry;
      const covered = register.top >= panel.top && register.bottom <= panel.bottom && register.left >= panel.left && register.right <= panel.right;
      expect(covered).toBe(!docked);
    });
  }
}

for (const theme of THEMES) {
  test(`the exchange puts each message on its author's side, labelled by its author, ${theme}`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: theme });
    await page.setViewportSize({ width: 1920, height: 1032 });
    const board = loadBoard("scene-c");
    await openScene(page, board);
    await openFile(page, named(board, MARKUP_SESSION));

    const list = exchange(page);
    // The messages are the list's own items; a message's markdown lists are inside them.
    await expect(list.locator(":scope > li")).toHaveCount(2);
    const human = list.locator('[data-author="human"]');
    const agent = list.locator('[data-author="agent"]');
    // Read in order by assistive technology: the author's label, then the text.
    expect(await human.evaluate((li) => li.textContent!.trim().startsWith("Your last prompt"))).toBe(true);
    expect(await agent.evaluate((li) => li.textContent!.trim().startsWith("Claude's last answer"))).toBe(true);

    const listBox = (await list.boundingBox())!;
    const humanBox = (await human.boundingBox())!;
    const agentBox = (await agent.boundingBox())!;
    // The author's on the right, Claude's on the left, neither across.
    expect(Math.abs(humanBox.x + humanBox.width - (listBox.x + listBox.width))).toBeLessThanOrEqual(1);
    expect(humanBox.x).toBeGreaterThan(listBox.x + 8);
    expect(Math.abs(agentBox.x - listBox.x)).toBeLessThanOrEqual(1);
    expect(agentBox.x + agentBox.width).toBeLessThan(listBox.x + listBox.width - 8);
    expect(humanBox.y).toBeLessThan(agentBox.y);

    // Two bubble styles, each distinct from the panel's surface.
    const fills = await page.evaluate(() =>
      ["[data-author=human]", "[data-author=agent]", "[data-testid=session-file]"].map((s) => getComputedStyle(document.querySelector(s)!).backgroundColor),
    );
    expect(new Set(fills).size).toBe(3);
  });
}

// Story 4.20 item 6 (the owner's revision of D-706, D-1045): the messages
// read as markdown, through a safe subset written as text nodes. Markup is
// shown as written, no link is made and no image is loaded.
test("the exchange renders its markdown, markup shown as written, no link and no image", async ({ page }) => {
  await page.setViewportSize({ width: 1920, height: 1032 });
  const board = loadBoard("scene-c");
  const card = named(board, MARKUP_SESSION);
  // The fixture holds what the test is about: markup and markdown in the
  // prompt, a markdown answer the hook kept as head and tail.
  expect(card.last_prompt).toContain("<img");
  expect(card.last_prompt).toContain("**");
  expect(card.last_message).toContain("## ");
  expect(card.last_message).toContain("<b>");
  expect(card.last_message_tail).toContain("| --- |");
  expect(card.last_message_tail).toContain("<img");
  await openScene(page, board);
  await openFile(page, card);

  const prompt = page.getByTestId("file-prompt");
  const answer = page.getByTestId("file-answer");
  const tail = page.getByTestId("file-answer-tail");
  // The markdown, read: emphasis, code, a heading as a bold line, a list, a
  // table as a table.
  await expect(prompt.locator("strong")).toHaveText("install.md");
  await expect(answer.locator("p.heading strong")).toHaveText("Link fixes");
  await expect(answer.locator("h1, h2, h3, h4, h5, h6")).toHaveCount(0);
  await expect(answer.locator("p:not(.heading) strong")).toHaveText("14");
  await expect(answer.locator("code").first()).toHaveText("install.md");
  await expect(answer.locator("ul > li")).toHaveCount(4);
  await expect(tail.locator("th")).toHaveText(["file", "links"]);
  await expect(tail.locator("tbody tr")).toHaveCount(2);
  await expect(tail.locator("em")).toHaveText("run the link checker again");
  // A link is its text and its address, never an anchor.
  await expect(prompt.locator("[data-md-link]")).toHaveText("docs (https://example.com/docs)");
  // Markup shown as written, nothing of it parsed or run.
  await expect(answer).toContainText("<b>none</b> left.");
  await expect(answer).toContainText('<a href="https://example.com">home</a>: kept as it was');
  await expect(prompt).toContainText('<img src=x onerror="window.__officinaXss = 1">');
  await expect(tail).toContainText('The <img src=x onerror="window.__officinaXss = 1"> tag');
  await expect(file(page).locator("a, img, b, script, iframe")).toHaveCount(0);
  const tags = await exchange(page).evaluate((list) => [...new Set(Array.from(list.querySelectorAll("*")).map((e) => e.tagName))].sort());
  const subset = ["BLOCKQUOTE", "BR", "CODE", "DIV", "EM", "HR", "LI", "OL", "P", "PRE", "SPAN", "STRONG", "TABLE", "TBODY", "TD", "TH", "THEAD", "TR", "UL"];
  for (const tag of tags) expect(subset).toContain(tag);
  // The hook's cut is kept: the head ends and the tail starts with its
  // ellipsis.
  expect((await answer.textContent())!.trimEnd().endsWith("…")).toBe(true);
  expect((await tail.textContent())!.trimStart().startsWith("…")).toBe(true);
  expect(await page.evaluate(() => (window as { __officinaXss?: number }).__officinaXss)).toBeUndefined();
});

test("a wide table scrolls inside its bubble", async ({ page }) => {
  await page.setViewportSize({ width: 1920, height: 1032 });
  const board = structuredClone(loadBoard("scene-c"));
  const card = named(board, MARKUP_SESSION);
  const columns = 12;
  card.last_message_tail = [
    `| ${Array.from({ length: columns }, (_, i) => `column-${i}`).join(" | ")} |`,
    `| ${Array.from({ length: columns }, () => "---").join(" | ")} |`,
    `| ${Array.from({ length: columns }, (_, i) => `value-${i}`).join(" | ")} |`,
  ].join("\n");
  await openScene(page, board);
  await openFile(page, card);
  const facts = await page.getByTestId("file-answer-tail").evaluate((tail) => {
    const box = tail.querySelector<HTMLElement>("[data-md-table]")!;
    const bubble = tail.closest("li")!.getBoundingClientRect();
    const panel = document.querySelector("[data-testid=session-file]")!.getBoundingClientRect();
    return {
      scrolls: box.scrollWidth > box.clientWidth,
      overflowX: getComputedStyle(box).overflowX,
      boxRight: box.getBoundingClientRect().right,
      bubbleRight: bubble.right,
      panelRight: panel.right,
      panelScrolls: document.querySelector<HTMLElement>("[data-testid=session-file]")!.scrollWidth >
        document.querySelector<HTMLElement>("[data-testid=session-file]")!.clientWidth,
    };
  });
  expect(facts.scrolls).toBe(true);
  expect(facts.overflowX).toBe("auto");
  expect(facts.boxRight).toBeLessThanOrEqual(facts.bubbleRight);
  expect(facts.bubbleRight).toBeLessThanOrEqual(facts.panelRight);
  expect(facts.panelScrolls).toBe(false);
});

test("a side the card does not carry shows nothing", async ({ page }) => {
  const board = loadBoard("scene-c");
  const live = cards(board).filter((c) => c.scene !== null);
  const promptOnly = live.find((c) => c.last_prompt !== null && c.last_message === null && c.last_message_tail === null);
  const neither = live.find((c) => c.last_prompt === null && c.last_message === null && c.last_message_tail === null);
  expect(promptOnly, "a session with a prompt and no answer yet").toBeDefined();
  expect(neither, "a session with no prompt and no answer").toBeDefined();
  await openScene(page, board);

  await openFile(page, promptOnly!);
  await expect(exchange(page).locator(":scope > li")).toHaveCount(1);
  await expect(exchange(page).locator('[data-author="human"]')).toHaveCount(1);
  await expect(page.getByTestId("file-answer")).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(file(page)).toHaveCount(0);

  await openFile(page, neither!);
  await expect(page.getByTestId("file-exchange")).toHaveCount(0);
});
