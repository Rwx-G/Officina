// The meeting (story 5.10; FR-5, AD-84, AD-93, D-111, D-528, D-564): Receive
// on a Needs you or Your turn card, in the session file and on a subject to
// resume opens one panel in the file's slot, which resizes nothing and
// closes back to its opener; it shows each section of a meeting printed by
// the window's own functions (`orchestrator_fixture`, kind `meeting`), jumps
// to a file's hunks and to a failed run's journal entry, and asks again when
// the board shows its card changed. Its composer (story 5.11) sends the
// author's answer by its delivery, copying it on the same gesture. The
// orchestrator's latest recap for the session (story 5.17) shows under a
// neutral "proposal" label, as text.
import { expect, test, type Page } from "@playwright/test";
import type { Board } from "../src/lib/generated/Board";
import type { Card } from "../src/lib/generated/Card";
import type { MeetingView } from "../src/lib/generated/MeetingView";
import type { RelayView } from "../src/lib/generated/RelayView";
import { fixtureOf } from "./fixture-index";
import { calls, loadBoard, loadRegistry, loadRelay, openBoard, openScene, transitionsDone } from "./fixtures";

const background = (): MeetingView => fixtureOf<MeetingView>("meeting", "meeting-background");
const interactive = (): MeetingView => fixtureOf<MeetingView>("meeting", "meeting-interactive");
const panel = (page: Page) => page.getByTestId("meeting-panel");
const cardOf = (page: Page, key: string) => page.locator(`[data-testid="card"][data-key=${JSON.stringify(key)}]`);

/** The meetings the backend answers, by key: `view` under each key. */
async function serveMeetings(page: Page, byKey: Record<string, MeetingView>): Promise<void> {
  await page.evaluate((meetings) => (window.__officinaMeetings = meetings), byKey);
}

/** `view` as the meeting of `key`. */
const as = (view: MeetingView, key: string): MeetingView => ({ ...view, key });

const meetingCalls = async (page: Page) => (await calls(page)).filter((c) => c.cmd === "open_meeting").map((c) => (c.args as { key: string }).key);

test("Receive opens the panel from a Needs you card, a Your turn card, the file and a subject to resume", async ({ page, browser }) => {
  const board = loadBoard("busy");
  const needs = board.columns.needs_you[0]!;
  const turn = board.columns.your_turn[0]!;
  await openBoard(page, board);
  await serveMeetings(page, { [needs.key]: as(background(), needs.key), [turn.key]: as(interactive(), turn.key) });
  for (const card of [needs, turn]) {
    await cardOf(page, card.key).getByTestId("receive").click();
    await expect(panel(page)).toHaveAttribute("data-key", card.key);
    await expect(panel(page).getByTestId("meeting-decision")).toBeVisible();
    await panel(page).getByTestId("meeting-close").click();
    await expect(panel(page)).toHaveCount(0);
  }
  // A working card offers none.
  await expect(cardOf(page, board.columns.working[0]!.key).getByTestId("receive")).toHaveCount(0);

  // The session file, in Scene mode: the meeting takes its slot.
  const inScene = await (await browser.newContext()).newPage();
  const scene = loadBoard("scene-c");
  const live = Object.values(scene.columns)
    .flat()
    .find((c: Card) => c.scene !== null)!;
  await openScene(inScene, scene);
  await serveMeetings(inScene, { [live.key]: as(background(), live.key) });
  await inScene.getByTestId("scene").locator(`[data-layer="targets"] [data-key=${JSON.stringify(live.key)}]`).click();
  await inScene.clock.runFor(1_000);
  await inScene.getByTestId("file-receive").click();
  await expect(panel(inScene)).toHaveAttribute("data-key", live.key);
  await expect(inScene.getByTestId("session-file")).toHaveCount(0);

  // A subject to resume, from the Subjects view: the panel shows in the
  // Sessions view, on the subject's latest prompted session.
  const fromSubjects = await (await browser.newContext()).newPage();
  const registry = loadRegistry("subjects");
  const toResume = registry.subjects.find((s) => s.status === "to_resume")!;
  await openBoard(fromSubjects, loadBoard("subjects-board"), {}, null, registry);
  await serveMeetings(fromSubjects, { [`subject:${toResume.id}`]: as(background(), "sid:00000000-0000-0000-0000-000000000601") });
  await fromSubjects.getByTestId("view-subjects").click();
  await fromSubjects.locator(`[data-testid="subject-card"][data-subject="${toResume.id}"]`).getByTestId("subject-receive").click();
  await expect(panel(fromSubjects)).toBeVisible();
  await expect(fromSubjects.getByTestId("subjects-view")).toHaveCount(0);
  expect(await meetingCalls(fromSubjects)).toContain(`subject:${toResume.id}`);
});

test("opening the panel resizes nothing and Close returns focus", async ({ page }) => {
  const board = loadBoard("busy");
  const needs = board.columns.needs_you[0]!;
  await openBoard(page, board);
  await serveMeetings(page, { [needs.key]: as(background(), needs.key) });
  const measure = () =>
    page.evaluate(() => {
      const main = document.querySelector("main")!.getBoundingClientRect();
      return { w: main.width, h: main.height, x: main.x, scroll: document.documentElement.scrollWidth };
    });
  const before = await measure();
  const receive = cardOf(page, needs.key).getByTestId("receive");
  await receive.click();
  await expect(panel(page).getByTestId("meeting-changes")).toBeVisible();
  expect(await measure()).toEqual(before);
  const box = (await panel(page).boundingBox())!;
  expect(Math.round(box.width)).toBe(420);
  await expect(panel(page).getByTestId("meeting-close")).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(panel(page)).toHaveCount(0);
  await expect(receive).toBeFocused();
  await receive.click();
  await panel(page).getByTestId("meeting-close").click();
  await expect(receive).toBeFocused();
  expect(await measure()).toEqual(before);
});

for (const theme of ["light", "dark"] as const) {
  test(`the panel reads the same in both themes but for its tokens, ${theme}`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: theme });
    const board = loadBoard("busy");
    const needs = board.columns.needs_you[0]!;
    await openBoard(page, board);
    await serveMeetings(page, { [needs.key]: as(background(), needs.key) });
    await cardOf(page, needs.key).getByTestId("receive").click();
    await expect(panel(page).getByTestId("diff-text")).toBeVisible();
    // The recap's label and texts are among the colours checked.
    await expect(panel(page).getByTestId("meeting-recap-label")).toBeVisible();
    await transitionsDone(page);
    const found = await page.evaluate(() => {
      const root = getComputedStyle(document.documentElement);
      const probe = document.createElement("span");
      document.body.append(probe);
      const resolve = (token: string) => {
        probe.style.color = root.getPropertyValue(token).trim();
        return getComputedStyle(probe).color;
      };
      const tokens = Object.fromEntries(
        ["--color-text", "--color-text-muted", "--color-working", "--color-surface", "--color-bg", "--color-border", "--color-attention", "--color-danger"].map((t) => [
          t,
          resolve(t),
        ]),
      );
      probe.remove();
      const panel = document.querySelector('[data-testid="meeting-panel"]')!;
      const colours = new Set<string>();
      for (const el of [panel, ...Array.from(panel.querySelectorAll("*"))]) {
        const style = getComputedStyle(el);
        colours.add(style.color);
        if (style.backgroundColor !== "rgba(0, 0, 0, 0)") colours.add(style.backgroundColor);
      }
      return { tokens, colours: [...colours], background: getComputedStyle(panel).backgroundColor, diff: getComputedStyle(panel.querySelector('[data-testid="diff-text"]')!).color };
    });
    expect(found.background).toBe(found.tokens["--color-surface"]);
    expect(found.diff).toBe(found.tokens["--color-text"]);
    const allowed = ["--color-text", "--color-text-muted", "--color-working", "--color-surface", "--color-bg"].map((t) => found.tokens[t]);
    for (const colour of found.colours) expect(allowed, `${theme}: ${colour}`).toContain(colour);
    expect(found.colours).not.toContain(found.tokens["--color-attention"]);
  });
}

test("the panel shows each section from the fixture", async ({ page }) => {
  const board = loadBoard("busy");
  const needs = board.columns.needs_you[0]!;
  const view = as(background(), needs.key);
  if (view.changes.state !== "read" || view.logs?.state !== "shown") throw new Error("the background meeting reads changes and logs");
  await openBoard(page, board);
  await serveMeetings(page, { [needs.key]: view });
  await cardOf(page, needs.key).getByTestId("receive").click();
  const p = panel(page);
  await expect(p.getByRole("heading", { level: 2 })).toHaveText(view.name);
  await expect(p.getByTestId("meeting-notification")).toHaveText(view.decision.notification!);
  await expect(p.getByTestId("meeting-answer-end")).toHaveText(view.decision.answer_end!);
  await expect(p.getByTestId("meeting-last-prompt")).toHaveText(view.decision.last_prompt!);
  await expect(p.getByTestId("meeting-delivery")).toHaveAttribute("data-delivery", view.delivery);
  await expect(p.getByTestId("meeting-run")).toHaveCount(view.runs.length);
  for (const result of ["succeeded", "failed", "refused"]) await expect(p.locator(`[data-testid="meeting-run"][data-result="${result}"]`)).toHaveCount(1);
  await expect(p.getByTestId("meeting-commit")).toHaveCount(view.changes.commits.length);
  await expect(p.getByTestId("meeting-commit").first()).toContainText(view.changes.commits[0]!.subject);
  await expect(p.getByTestId("diff-file")).toHaveCount(view.changes.diff.files.length);
  await expect(p.getByTestId("meeting-untracked")).toContainText(view.changes.untracked[0]!.path);
  await expect(p.getByTestId("meeting-logs-text")).toContainText(view.logs.text.split("\n")[0]!);
  await expect(p.getByTestId("meeting-first-error")).toBeVisible();
  // An interactive session shows no logs and offers Focus.
  const turn = board.columns.your_turn[0]!;
  await serveMeetings(page, { [turn.key]: as(interactive(), turn.key) });
  await p.getByTestId("meeting-close").click();
  await cardOf(page, turn.key).getByTestId("receive").click();
  await expect(p.getByTestId("meeting-logs")).toHaveCount(0);
  await expect(p.getByTestId("meeting-focus")).toBeVisible();
});

test("the latest recap shows under its proposal label, as text, after the runs", async ({ page }) => {
  const board = loadBoard("busy");
  const needs = board.columns.needs_you[0]!;
  const view = as(background(), needs.key);
  if (!view.recap?.content || !("recap" in view.recap.content)) throw new Error("the background meeting has a recap");
  const recap = view.recap.content.recap;
  await openBoard(page, board);
  await serveMeetings(page, { [needs.key]: view, [board.columns.your_turn[0]!.key]: as(interactive(), board.columns.your_turn[0]!.key) });
  await cardOf(page, needs.key).getByTestId("receive").click();
  const section = panel(page).getByTestId("meeting-recap");
  await expect(section.getByTestId("meeting-recap-label")).toHaveText("proposal");
  await expect(section.getByTestId("meeting-recap-done")).toHaveText(recap.done);
  await expect(section.getByTestId("meeting-recap-blocking")).toHaveText(recap.blocking);
  await expect(section.getByTestId("meeting-recap-decision")).toHaveText(recap.decision_expected);
  // The fixture's done text is markup: it renders as text and runs nothing.
  expect(recap.done).toContain("<img");
  await expect(section.locator("img")).toHaveCount(0);
  expect(await page.evaluate(() => (window as { __officinaXss?: number }).__officinaXss)).toBeUndefined();
  // AD-84's order: runs and approvals, then the recap, then the changes.
  const order = await panel(page).evaluate((el) => Array.from(el.querySelectorAll("section"), (s) => s.dataset.testid));
  expect(order.indexOf("meeting-recap")).toBe(order.indexOf("meeting-runs") + 1);
  expect(order.indexOf("meeting-changes")).toBe(order.indexOf("meeting-recap") + 1);
  // A proposal, never the author's answer.
  await expect(panel(page).getByTestId("answer-text")).toHaveValue("");
  // A meeting without a recap shows no section.
  await panel(page).getByTestId("meeting-close").click();
  await cardOf(page, board.columns.your_turn[0]!.key).getByTestId("receive").click();
  await expect(panel(page).getByTestId("meeting-decision")).toBeVisible();
  await expect(panel(page).getByTestId("meeting-recap")).toHaveCount(0);
});

test("a file jumps to its hunks and a failed run to its journal entry", async ({ page }) => {
  const board = loadBoard("busy");
  const needs = board.columns.needs_you[0]!;
  const relay: RelayView = loadRelay("relay-activity");
  const failed = relay.recent.find((e) => e.event.kind === "run" && e.tone === "failed" && e.event.state === "finished")!;
  if (failed.event.kind !== "run") throw new Error("a failed run");
  const runId = failed.event.started.run_id;
  const base = as(background(), needs.key);
  const view: MeetingView = { ...base, runs: base.runs.map((r) => (r.result === "failed" ? { ...r, run_id: runId } : r)) };
  if (view.changes.state !== "read") throw new Error("changes");
  await openBoard(page, board, {}, relay);
  await serveMeetings(page, { [needs.key]: view });
  await cardOf(page, needs.key).getByTestId("receive").click();
  const last = view.changes.diff.files.at(-1)!;
  await panel(page).getByTestId("diff-file").last().click();
  await expect(panel(page).locator(`[data-line="${last.line}"]`)).toBeFocused();
  await panel(page).getByRole("button", { name: `Hunk 1 of ${last.path}` }).click();
  await expect(panel(page).locator(`[data-line="${last.hunks[0]}"]`)).toBeFocused();
  await panel(page).getByTestId("meeting-first-error").click();
  await expect(panel(page).locator(`[data-log-line="${(view.logs as { first_error_line: number }).first_error_line}"]`)).toBeFocused();
  await panel(page).getByTestId("meeting-journal").click();
  await expect(page.getByTestId("relay-view")).toBeVisible();
  await expect(page.locator(`[data-run-id="${runId}"]`)).toBeFocused();
});

test("the panel asks again when the card's state changes", async ({ page }) => {
  const board = loadBoard("busy");
  const needs = board.columns.needs_you[0]!;
  await openBoard(page, board);
  await serveMeetings(page, { [needs.key]: as(background(), needs.key) });
  await cardOf(page, needs.key).getByTestId("receive").click();
  await expect(panel(page).getByTestId("meeting-decision")).toBeVisible();
  expect(await meetingCalls(page)).toEqual([needs.key]);
  // A board where nothing of that card changed asks nothing.
  await page.evaluate((b) => window.__officinaPushBoard!(b), { ...board, generated_at_ms: board.generated_at_ms + 1_000 });
  await page.clock.runFor(500);
  expect(await meetingCalls(page)).toEqual([needs.key]);
  // Its card leaves Needs you: the meeting is read again.
  const moved: Board = structuredClone(board);
  const card = moved.columns.needs_you.shift()!;
  moved.columns.working.unshift({ ...card, column: "working", waiting_for: null });
  await page.evaluate((b) => window.__officinaPushBoard!(b), moved);
  await expect.poll(() => meetingCalls(page)).toEqual([needs.key, needs.key]);
});

// ---- story 5.11: answers (FR-6, AD-85, D-168) ----

/** Opens the busy board's first Needs you card as a meeting whose
 *  delivery is `delivery` and whose session is `kind`, the clipboard
 *  readable. */
async function openAnswering(page: Page, delivery: MeetingView["delivery"], view: MeetingView = background()): Promise<MeetingView> {
  await page.context().grantPermissions(["clipboard-read", "clipboard-write"]);
  const board = loadBoard("busy");
  const needs = board.columns.needs_you[0]!;
  const meeting = { ...as(view, needs.key), delivery };
  await openBoard(page, board);
  await serveMeetings(page, { [needs.key]: meeting });
  await cardOf(page, needs.key).getByTestId("receive").click();
  await expect(panel(page).getByTestId("meeting-delivery")).toHaveAttribute("data-delivery", delivery);
  return meeting;
}

const answerCalls = async (page: Page) =>
  (await calls(page)).filter((c) => c.cmd === "send_answer").map((c) => c.args as { key: string; expected: string; text: string });

const clipboard = (page: Page) => page.evaluate(() => navigator.clipboard.readText());

test("an answer to a live background session is sent and confirmed", async ({ page }) => {
  const meeting = await openAnswering(page, "resume_live");
  const text = panel(page).getByTestId("answer-text");
  await expect(text).toHaveValue("");
  await text.fill("Keep the 1.x line, then run the tests again.");
  await panel(page).getByTestId("answer-send").click();
  await expect(panel(page).getByTestId("answer-result")).toHaveAttribute("data-outcome", "confirmed");
  expect(await answerCalls(page)).toEqual([{ key: meeting.key, expected: "resume_live", text: "Keep the 1.x line, then run the tests again." }]);
  expect(await clipboard(page)).toBe("Keep the 1.x line, then run the tests again.");
  // Confirmed, the draft goes.
  await expect(text).toHaveValue("");
});

test("an unconfirmed answer stays in the panel and on the clipboard", async ({ page }) => {
  await openAnswering(page, "resume_live");
  await page.evaluate(() => (window.__officinaAnswerResult = { outcome: "not_confirmed" }));
  const text = panel(page).getByTestId("answer-text");
  await text.fill("Use the fixture of the release branch.");
  // Ctrl+Enter sends from the keyboard: an answer approves nothing.
  await text.press("Control+Enter");
  await expect(panel(page).getByTestId("answer-result")).toHaveAttribute("data-outcome", "not_confirmed");
  await expect(panel(page).getByTestId("answer-result")).toContainText("Not confirmed");
  await expect(text).toHaveValue("Use the fixture of the release branch.");
  expect(await clipboard(page)).toBe("Use the fixture of the release branch.");
  // Closing and receiving again keeps the draft, in memory.
  await panel(page).getByTestId("meeting-close").click();
  await cardOf(page, loadBoard("busy").columns.needs_you[0]!.key).getByTestId("receive").click();
  await expect(panel(page).getByTestId("answer-text")).toHaveValue("Use the fixture of the release branch.");
  await panel(page).getByTestId("answer-discard").click();
  await expect(panel(page).getByTestId("answer-text")).toHaveValue("");
});

test("a waiting background session copies and opens attach", async ({ page }) => {
  await openAnswering(page, "copy_and_attach");
  await page.evaluate(() => (window.__officinaAnswerResult = { outcome: "attached" }));
  await panel(page).getByTestId("answer-text").fill("Pick the second option.");
  await panel(page).getByTestId("answer-send").click();
  await expect(panel(page).getByTestId("answer-result")).toHaveAttribute("data-outcome", "attached");
  expect((await answerCalls(page))[0]!.expected).toBe("copy_and_attach");
  expect(await clipboard(page)).toBe("Pick the second option.");
});

test("an ended session never seen by claude agents is copy only", async ({ page }) => {
  await openAnswering(page, "copy_only");
  await expect(panel(page).getByTestId("meeting-delivery")).toContainText("resume it by hand in its folder");
  await page.evaluate(() => (window.__officinaAnswerResult = { outcome: "copied" }));
  await panel(page).getByTestId("answer-text").fill("Carry on from the last commit.");
  await panel(page).getByTestId("answer-send").click();
  await expect(panel(page).getByTestId("answer-result")).toHaveAttribute("data-outcome", "copied");
  expect((await answerCalls(page))[0]!.expected).toBe("copy_only");
  expect(await clipboard(page)).toBe("Carry on from the last commit.");
});

test("a live interactive session offers Focus and no composer", async ({ page }) => {
  const board = loadBoard("busy");
  const turn = board.columns.your_turn[0]!;
  await openBoard(page, board);
  await serveMeetings(page, { [turn.key]: as(interactive(), turn.key) });
  await cardOf(page, turn.key).getByTestId("receive").click();
  await expect(panel(page).getByTestId("meeting-focus")).toBeVisible();
  await expect(panel(page).getByTestId("answer-composer")).toHaveCount(0);
  await panel(page).getByTestId("meeting-focus").click();
  expect((await calls(page)).some((c) => c.cmd === "focus")).toBe(true);
});

// ---- story 5.21: answer proposals (AD-182, D-28, D-488, D-564, D-575) ----

/** The background meeting's proposed answer, from the fixture. */
function proposedOf(view: MeetingView): { id: string; text: string; rationale: string } {
  const proposal = view.proposed_answer;
  if (!proposal?.content || !("answer" in proposal.content)) throw new Error("the background meeting has a proposed answer");
  return { id: proposal.id, ...proposal.content.answer };
}

const proposalCalls = async (page: Page) =>
  (await calls(page)).filter((c) => c.cmd === "send_answer").map((c) => c.args as Record<string, unknown>);

for (const theme of ["light", "dark"] as const) {
  test(`a pending answer proposal shows under its label with its rationale, ${theme}`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: theme });
    const meeting = await openAnswering(page, "resume_live");
    const proposed = proposedOf(meeting);
    const slot = panel(page).getByTestId("meeting-proposal-slot");
    await expect(slot.getByTestId("meeting-proposal-label")).toHaveText("proposal");
    await expect(slot.getByTestId("meeting-proposal-text")).toHaveText(proposed.text);
    await expect(slot.getByTestId("meeting-proposal-rationale")).toHaveText(proposed.rationale);
    // The fixture's text is markup: it renders as text and runs nothing.
    expect(proposed.text).toContain("<img");
    await expect(slot.locator("img")).toHaveCount(0);
    expect(await page.evaluate(() => (window as { __officinaXss?: number }).__officinaXss)).toBeUndefined();
    // Beside the composer, which stays empty: nothing is copied into it.
    await expect(panel(page).getByTestId("answer-text")).toHaveValue("");
    // Neutral: the slot takes the panel's text, muted and surface tokens,
    // never the attention colour.
    await transitionsDone(page);
    const found = await slot.evaluate((el) => {
      const root = getComputedStyle(document.documentElement);
      const probe = document.createElement("span");
      document.body.append(probe);
      const resolve = (token: string) => {
        probe.style.color = root.getPropertyValue(token).trim();
        return getComputedStyle(probe).color;
      };
      const tokens = Object.fromEntries(
        ["--color-text", "--color-text-muted", "--color-surface", "--color-bg", "--color-attention"].map((t) => [t, resolve(t)]),
      );
      probe.remove();
      const colours = new Set<string>();
      for (const e of [el, ...Array.from(el.querySelectorAll("*"))]) {
        const style = getComputedStyle(e);
        colours.add(style.color);
        if (style.backgroundColor !== "rgba(0, 0, 0, 0)") colours.add(style.backgroundColor);
      }
      return { tokens, colours: [...colours] };
    });
    const allowed = ["--color-text", "--color-text-muted", "--color-surface", "--color-bg"].map((t) => found.tokens[t]);
    for (const colour of found.colours) expect(allowed, `${theme}: ${colour}`).toContain(colour);
    expect(found.colours).not.toContain(found.tokens["--color-attention"]);
  });

  test(`Send the proposed answer needs a pointer click, ${theme}`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: theme });
    const meeting = await openAnswering(page, "resume_live");
    const proposed = proposedOf(meeting);
    await panel(page).getByTestId("proposal-send").click();
    await expect(panel(page).getByTestId("meeting-proposal-result")).toHaveAttribute("data-outcome", "confirmed");
    // Its id goes to the backend, never a text (D-575); the same click
    // copies the text.
    expect(await proposalCalls(page)).toEqual([{ key: meeting.key, expected: "resume_live", proposalId: proposed.id }]);
    expect(await clipboard(page)).toBe(proposed.text);
    // Sent, it leaves the meeting read again, its result kept.
    await expect(panel(page).getByTestId("meeting-proposal-none")).toBeVisible();
    await expect(panel(page).getByTestId("answer-text")).toHaveValue("");
  });

  test(`a keyboard press never sends a proposal, ${theme}`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: theme });
    await openAnswering(page, "resume_live");
    const send = panel(page).getByTestId("proposal-send");
    await send.focus();
    await page.keyboard.press("Enter");
    await page.keyboard.press("Space");
    await send.dispatchEvent("click", { detail: 0 });
    await page.waitForTimeout(200);
    expect(await proposalCalls(page)).toEqual([]);
    await expect(panel(page).getByTestId("meeting-proposal-result")).toHaveCount(0);
  });

  test(`dismiss sends dismiss_proposal, ${theme}`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: theme });
    const meeting = await openAnswering(page, "resume_live");
    const proposed = proposedOf(meeting);
    const slot = panel(page).getByTestId("meeting-proposal-slot");
    const before = (await slot.boundingBox())!;
    await panel(page).getByTestId("meeting-proposal-dismiss").click();
    await expect(panel(page).getByTestId("meeting-proposal-none")).toBeVisible();
    const dismissed = (await calls(page)).filter((c) => c.cmd === "dismiss_proposal").map((c) => c.args);
    expect(dismissed).toEqual([{ proposalId: proposed.id }]);
    expect(await proposalCalls(page)).toEqual([]);
    // Its going moves nothing: the slot keeps its box.
    expect(await slot.boundingBox()).toEqual(before);
  });
}

test("no proposal slot for a delivery a proposal never takes", async ({ page }) => {
  await openAnswering(page, "copy_and_attach");
  await expect(panel(page).getByTestId("answer-composer")).toBeVisible();
  await expect(panel(page).getByTestId("meeting-proposal-slot")).toHaveCount(0);
});
