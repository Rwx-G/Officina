// Story 3.7: the Triage panel (FR-9, AD-55, D-61) on the registry view the
// real `officina subjects --json` printed after the `registry_fixture`
// example wrote the `triage` scenario through the window's writer (D-72):
// a subject of two ended sessions, a background session proposed from its
// state, a session cut mid-turn under a subject named with markup, an empty
// subject, an unfiled session with one obvious subject and one with none.
import { expect, test, type Page } from "@playwright/test";
import { calls, loadBoard, loadRegistry, openBoard } from "./fixtures";
import {
  AUTHOR_REFUSAL_TEXT,
  CONFIRM_REFUSAL_TEXT,
  NOT_NEEDED_TEXT,
  PROPOSAL_SOURCE_TEXT,
  PROPOSED_VERDICT_TEXT,
  REASON_TEXT,
  TRIAGE_OUTCOME_TEXT,
} from "../src/lib/registry-text";
import type { ConfirmItem } from "../src/lib/generated/ConfirmItem";
import type { RegistryView } from "../src/lib/generated/RegistryView";
import type { TriageRow } from "../src/lib/generated/TriageRow";

async function openTriage(page: Page, view: RegistryView = loadRegistry("triage")): Promise<void> {
  await openBoard(page, loadBoard("triage-board"), {}, null, view);
  await page.getByTestId("view-subjects").click();
  await expect(page.getByTestId("triage-panel")).toBeVisible();
}

function rowOf(page: Page, row: TriageRow) {
  return page.locator(`[data-testid="triage-row"][data-key="${row.key}"]`);
}

/** The focusable element holding `row`, which takes the panel's keys. */
function holder(page: Page, row: TriageRow) {
  return page.locator(`[data-row="${row.key}"]`);
}

/** The items of every `confirm_triage` call, in order. */
async function confirmed(page: Page): Promise<ConfirmItem[][]> {
  return (await calls(page)).filter((c) => c.cmd === "confirm_triage").map((c) => (c.args as { items: ConfirmItem[] }).items);
}

async function focused(page: Page): Promise<string | null> {
  return page.evaluate(() => document.activeElement?.getAttribute("data-row") ?? null);
}

function find(view: RegistryView, matches: (row: TriageRow) => boolean, what: string): TriageRow {
  const found = view.triage.find(matches);
  if (!found) throw new Error(`no ${what} in the triage fixture: rebuild it with pnpm e2e:fixtures`);
  return found;
}

const obvious = (view: RegistryView) =>
  find(view, (r) => r.kind === "unfiled_session" && r.proposed_subject !== null, "unfiled row with a proposed subject");
const homeless = (view: RegistryView) =>
  find(view, (r) => r.kind === "unfiled_session" && r.proposed_subject === null, "unfiled row without a subject");

/** The view with `row` made an `unsure` model proposal (D-59). */
function withUnsure(view: RegistryView, row: TriageRow): RegistryView {
  return {
    ...view,
    triage: view.triage.map((r) =>
      r.key === row.key ? { ...r, proposal: { verdict: "unsure", source: "model", reason: "model", call_id: 9 }, verdict: null } : r,
    ),
  };
}

test.describe("Triage panel", () => {
  test("each row shows its proposal, source, reason and excerpts", async ({ page }) => {
    const view = loadRegistry("triage");
    await openTriage(page, view);
    const panel = page.getByTestId("triage-panel");
    const subjectsTitle = page.locator("#subjects-title");
    expect((await panel.boundingBox())!.y).toBeLessThan((await subjectsTitle.boundingBox())!.y);
    await expect(page.getByTestId("triage-row")).toHaveCount(view.triage.length);
    for (const [i, row] of view.triage.entries()) {
      const shown = rowOf(page, row);
      await expect(page.getByTestId("triage-row").nth(i)).toHaveAttribute("data-key", row.key);
      await expect(shown.getByTestId("triage-proposed")).toHaveText(PROPOSED_VERDICT_TEXT[row.proposal.verdict]);
      await expect(shown.getByTestId("triage-source")).toHaveText(PROPOSAL_SOURCE_TEXT[row.proposal.source]);
      await expect(shown.getByTestId("triage-reason")).toHaveText(REASON_TEXT[row.proposal.reason]);
      if (row.excerpts?.prompt) await expect(shown.getByTestId("triage-prompt")).toHaveText(row.excerpts.prompt);
      const answer = row.excerpts?.answer_tail ?? row.excerpts?.answer_head;
      if (answer) await expect(shown.getByTestId("triage-answer")).toHaveText(answer);
    }
    const sources = new Set(view.triage.map((r) => r.proposal.source));
    expect(sources.has("state") && sources.has("rules")).toBe(true);
  });

  test("an unfiled row proposes the obvious subject and lets me choose or name one", async ({ page }) => {
    const view = loadRegistry("triage");
    await openTriage(page, view);
    const row = obvious(view);
    const proposed = row.choices.find((c) => c.id === row.proposed_subject)!;
    const shown = rowOf(page, row);
    await expect(shown.getByTestId("triage-file-under")).toHaveText(`Files under ${proposed.name}`);
    await expect(shown.locator(`[data-testid="pick-subject"][data-subject="${proposed.id}"]`)).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await shown.getByTestId("picker-new-name").fill("invoice rounding");
    await shown.getByTestId("picker-create").click();
    await expect(shown.getByTestId("triage-file-under")).toHaveText("Files under a new subject: invoice rounding");
    await shown.getByTestId("triage-confirm").click();
    await expect.poll(() => confirmed(page)).toEqual([
      [
        {
          key: row.key,
          shown: row.proposal,
          verdict: row.verdict,
          routine: false,
          file_under: { new: { name: "invoice rounding" } },
        },
      ],
    ]);

    const none = rowOf(page, homeless(view));
    await expect(none.getByTestId("triage-file-under")).toHaveText("No subject chosen yet.");
    await expect(none.getByTestId("pick-subject")).toHaveCount(0);
    await expect(none.getByTestId("triage-confirm")).toBeDisabled();
  });

  test("an empty subject is offered for archiving", async ({ page }) => {
    const view = loadRegistry("triage");
    await openTriage(page, view);
    const row = find(view, (r) => r.kind === "empty_subject", "empty subject");
    const shown = rowOf(page, row);
    await expect(shown.getByTestId("triage-empty")).toHaveText("empty subject: archive?");
    await expect(shown.getByTestId("triage-proposed")).toHaveText(PROPOSED_VERDICT_TEXT.finished);
    await expect(shown.getByTestId("triage-verdict-finished")).toHaveAttribute("aria-pressed", "true");
    await shown.getByTestId("triage-confirm").click();
    await expect
      .poll(() => confirmed(page))
      .toEqual([[{ key: row.key, shown: row.proposal, verdict: "finished", routine: false, file_under: null }]]);
  });

  test("arrows move, f and t set the verdict, Enter confirms the focused row only", async ({ page }) => {
    const view = loadRegistry("triage");
    await openTriage(page, view);
    const [first, second] = view.triage;
    await holder(page, first!).focus();
    await page.keyboard.press("ArrowDown");
    expect(await focused(page)).toBe(second!.key);
    await page.keyboard.press("ArrowUp");
    expect(await focused(page)).toBe(first!.key);
    await page.keyboard.press("ArrowDown");
    const shown = rowOf(page, second!);
    await page.keyboard.press("f");
    await expect(shown.getByTestId("triage-verdict-finished")).toHaveAttribute("aria-pressed", "true");
    await page.keyboard.press("t");
    await expect(shown.getByTestId("triage-verdict-to_resume")).toHaveAttribute("aria-pressed", "true");
    await expect(rowOf(page, first!).getByTestId("triage-verdict-to_resume")).toHaveAttribute(
      "aria-pressed",
      String(first!.verdict === "to_resume"),
    );
    await page.keyboard.press("Enter");
    await expect.poll(() => confirmed(page)).toEqual([
      [{ key: second!.key, shown: second!.proposal, verdict: "to_resume", routine: false, file_under: null }],
    ]);
  });

  test("a held Enter confirms once", async ({ page }) => {
    const view = loadRegistry("triage");
    const row = view.triage[0]!;
    await openTriage(page, view);
    // Refused, the row is free again at once: only the repeat flag can stop
    // a held key from confirming it again.
    await page.evaluate((key) => (window.__officinaConfirmOutcome = { [key]: { kind: "refused", refusal: "proposal_changed" } }), row.key);
    await holder(page, row).focus();
    await page.keyboard.down("Enter");
    await expect(rowOf(page, row).getByTestId("triage-error")).toBeVisible();
    await page.keyboard.down("Enter");
    await page.keyboard.down("Enter");
    await page.keyboard.up("Enter");
    expect(await confirmed(page)).toHaveLength(1);
    // Positive control: a fresh press confirms again.
    await page.keyboard.press("Enter");
    await expect.poll(async () => (await confirmed(page)).length).toBe(2);
  });

  test("focus goes to the next row once the view drops the confirmed one", async ({ page }) => {
    const view = loadRegistry("triage");
    await openTriage(page, view);
    const [, middle, next] = view.triage;
    await holder(page, middle!).focus();
    await page.keyboard.press("Enter");
    await expect(rowOf(page, middle!).getByTestId("triage-in-flight")).toBeVisible();
    // Enter waits for the view that removes the row.
    await page.keyboard.press("Enter");
    expect(await confirmed(page)).toHaveLength(1);
    expect(await focused(page)).toBe(middle!.key);
    const dropped = { ...view, triage: view.triage.filter((r) => r.key !== middle!.key), to_sort: view.to_sort - 1 };
    await page.evaluate(async (next) => window.__officinaPushRegistry?.(next), dropped);
    await expect(rowOf(page, middle!)).toHaveCount(0);
    await expect.poll(() => focused(page)).toBe(next!.key);
  });

  test("an unsure row needs f or t before Enter", async ({ page }) => {
    const base = loadRegistry("triage");
    const row = base.triage[0]!;
    const view = withUnsure(base, row);
    await openTriage(page, view);
    const shown = rowOf(page, row);
    await expect(shown.getByTestId("triage-proposed")).toHaveText(PROPOSED_VERDICT_TEXT.unsure);
    await expect(shown.getByTestId("triage-no-verdict")).toBeVisible();
    await holder(page, row).focus();
    await page.keyboard.press("Enter");
    await expect(shown.getByTestId("triage-confirm")).toBeDisabled();
    expect(await confirmed(page)).toEqual([]);
    await page.keyboard.press("f");
    await page.keyboard.press("Enter");
    await expect.poll(() => confirmed(page)).toEqual([
      [
        {
          key: row.key,
          shown: { verdict: "unsure", source: "model", reason: "model", call_id: 9 },
          verdict: "finished",
          routine: false,
          file_under: row.kind === "unfiled_session" ? { subject: { id: row.proposed_subject! } } : null,
        },
      ],
    ]);
  });

  test("confirm all shown confirms the shown rows with one click", async ({ page }) => {
    const view = loadRegistry("triage");
    await openTriage(page, view);
    const left = homeless(view);
    await page.getByTestId("confirm-all").click();
    const sent = await confirmed(page);
    expect(sent).toHaveLength(1);
    expect(sent[0]!.map((item) => item.key)).toEqual(view.triage.filter((r) => r.key !== left.key).map((r) => r.key));
  });

  test("its label counts the rows and the ones left out", async ({ page }) => {
    const base = loadRegistry("triage");
    const unsure = base.triage.find((r) => r.kind !== "unfiled_session")!;
    const view = withUnsure(base, unsure);
    await openTriage(page, view);
    const all = page.getByTestId("confirm-all");
    const total = view.triage.length;
    await expect(all).toHaveText(`Confirm all ${total - 2} shown; left out: 1 without a verdict, 1 without a subject`);
    const pending = view.triage.find((r) => r.key !== unsure.key && r.key !== homeless(view).key)!;
    await holder(page, pending).focus();
    await page.keyboard.press("Enter");
    await expect(all).toHaveText(
      `Confirm all ${total - 3} shown; left out: 1 without a verdict, 1 without a subject, 1 being confirmed`,
    );
  });

  test("the to-sort chip is neutral and opens the panel", async ({ page }) => {
    const view = loadRegistry("triage");
    await openBoard(page, loadBoard("triage-board"), {}, null, view);
    const chip = page.getByTestId("to-sort-chip");
    await expect(chip).toHaveText(`${view.to_sort} to sort`);
    const colours = await chip.evaluate((element) => {
      const probe = document.createElement("div");
      probe.style.background = "var(--color-attention)";
      document.body.append(probe);
      const amber = getComputedStyle(probe).backgroundColor;
      probe.remove();
      return { chip: getComputedStyle(element).backgroundColor, amber };
    });
    expect(colours.amber).not.toBe("");
    expect(colours.chip).not.toBe(colours.amber);
    await expect(page.getByTestId("subjects-view")).toHaveCount(0);
    await chip.click();
    await expect(page.getByTestId("triage-panel")).toBeVisible();
    await expect.poll(() => focused(page)).toBe(view.triage[0]!.key);
  });

  test("confirming finished offers the routine box", async ({ page }) => {
    const view = loadRegistry("triage");
    await openTriage(page, view);
    const row = find(view, (r) => r.kind === "subject" && r.verdict === "to_resume", "subject row proposing to_resume");
    const shown = rowOf(page, row);
    await expect(shown.getByTestId("triage-routine")).toHaveCount(0);
    await shown.getByTestId("triage-verdict-finished").click();
    const box = shown.getByTestId("triage-routine");
    await expect(box).not.toBeChecked();
    await expect(shown).toContainText("Routine, could have run alone");
    await box.check();
    await shown.getByTestId("triage-confirm").click();
    await expect
      .poll(() => confirmed(page))
      .toEqual([[{ key: row.key, shown: row.proposal, verdict: "finished", routine: true, file_under: null }]]);
  });

  test("a refused item keeps its row with the reason", async ({ page }) => {
    const view = loadRegistry("triage");
    await openTriage(page, view);
    const changed = view.triage.find((r) => r.kind === "subject")!;
    const filing = obvious(view);
    await page.evaluate(
      ([a, b]) =>
        (window.__officinaConfirmOutcome = {
          [a]: { kind: "refused", refusal: "proposal_changed" },
          [b]: { kind: "filing_refused", refusal: "archived" },
        }),
      [changed.key, filing.key],
    );
    await rowOf(page, changed).getByTestId("triage-confirm").click();
    await rowOf(page, filing).getByTestId("triage-confirm").click();
    await expect(rowOf(page, changed).getByTestId("triage-error")).toHaveText(CONFIRM_REFUSAL_TEXT.proposal_changed);
    await expect(rowOf(page, filing).getByTestId("triage-error")).toHaveText(AUTHOR_REFUSAL_TEXT.archived);
    await expect(page.getByTestId("triage-row")).toHaveCount(view.triage.length);
    await expect(rowOf(page, changed).getByTestId("triage-confirm")).toBeEnabled();
    await expect(rowOf(page, changed).getByTestId("triage-in-flight")).toHaveCount(0);
  });

  test("a session filed without its verdict says its subject's row decides", async ({ page }) => {
    const view = loadRegistry("triage");
    await openTriage(page, view);
    const row = obvious(view);
    await page.evaluate(
      ([key, subject]) =>
        (window.__officinaConfirmOutcome = { [key]: { kind: "verdict_not_needed", subject, cause: "not_latest" } }),
      [row.key, row.proposed_subject!] as const,
    );
    await rowOf(page, row).getByTestId("triage-confirm").click();
    await expect(page.getByTestId("triage-note")).toHaveText(
      `The session was filed, and its verdict left to its subject's own row: ${NOT_NEEDED_TEXT.not_latest}.`,
    );
    await expect(rowOf(page, row).getByTestId("triage-error")).toHaveCount(0);
  });

  test("excerpts and names render as text, never as HTML", async ({ page }) => {
    const view = loadRegistry("triage");
    const named = find(view, (r) => r.subject?.name.includes("<img") ?? false, "row whose subject name is markup");
    const prompted = find(view, (r) => r.excerpts?.prompt?.includes("<img") ?? false, "row whose prompt is markup");
    await openTriage(page, view);
    await expect(rowOf(page, named).getByTestId("triage-title")).toHaveText(named.subject!.name);
    await expect(rowOf(page, prompted).getByTestId("triage-prompt")).toHaveText(prompted.excerpts!.prompt!);
    await expect(page.getByTestId("triage-panel").locator("img")).toHaveCount(0);
    expect(await page.evaluate(() => (window as { __officinaXss?: number }).__officinaXss)).toBeUndefined();
  });
});

// Story 3.11: rows and the triage model (AD-40, AD-52, D-87) on the `model`
// fixture, where the model is on, the latest row was answered with a reason
// that is markup, and the others wait for it; and on `model-paused`, where
// three calls then gave no answer.
test.describe("Model proposals on rows", () => {
  async function openModel(page: Page, view: RegistryView): Promise<void> {
    await openBoard(page, loadBoard("model-board"), {}, null, view);
    await page.getByTestId("view-subjects").click();
    await expect(page.getByTestId("triage-panel")).toBeVisible();
  }

  test("a queued row says model pending, then shows the model's proposal", async ({ page }) => {
    const view = loadRegistry("model");
    const pending = find(view, (r) => r.model_pending, "row pending for the model");
    await openModel(page, view);
    const row = rowOf(page, pending);
    await expect(row.getByTestId("triage-model-pending")).toHaveText("model pending");
    await expect(row.getByTestId("triage-source")).toHaveText(PROPOSAL_SOURCE_TEXT.rules);
    // The answer comes with the next view: the model's proposal and reason.
    const answered: RegistryView = {
      ...view,
      triage: view.triage.map((r) =>
        r.key === pending.key
          ? {
              ...r,
              model_pending: false,
              model_reason: "The tests pass and the change shipped.",
              proposal: { verdict: "finished", source: "model", reason: "model", call_id: 99 },
              verdict: "finished",
            }
          : r,
      ),
    };
    await page.evaluate((next) => window.__officinaPushRegistry?.(next), answered);
    await expect(row.getByTestId("triage-model-pending")).toHaveCount(0);
    await expect(row.getByTestId("triage-source")).toHaveText(PROPOSAL_SOURCE_TEXT.model);
    await expect(row.getByTestId("triage-proposed")).toHaveText(PROPOSED_VERDICT_TEXT.finished);
    await expect(row.getByTestId("triage-reason")).toHaveText(REASON_TEXT.model);
    await expect(row.getByTestId("triage-model-text")).toHaveText("The tests pass and the change shipped.");
  });

  test("model text renders as text under a model label", async ({ page }) => {
    const view = loadRegistry("model");
    const answered = find(view, (r) => r.proposal.source === "model", "row the model answered");
    expect(answered.model_reason).toContain("<img");
    await openModel(page, view);
    const row = rowOf(page, answered);
    await expect(row.getByTestId("triage-model-label")).toHaveText("model");
    await expect(row.getByTestId("triage-model-text")).toHaveText(answered.model_reason!);
    await expect(row.locator("img")).toHaveCount(0);
    expect(await page.evaluate(() => (window as unknown as { __officinaXss?: number }).__officinaXss)).toBeUndefined();
  });

  test("a call that gave no answer keeps the default and says why", async ({ page }) => {
    const view = loadRegistry("model-paused");
    const failed = find(view, (r) => r.model_call !== null, "row whose call gave no answer");
    await openModel(page, view);
    const row = rowOf(page, failed);
    await expect(row.getByTestId("triage-source")).toHaveText(PROPOSAL_SOURCE_TEXT.rules);
    await expect(row.getByTestId("triage-model-call")).toContainText(TRIAGE_OUTCOME_TEXT[failed.model_call!.outcome]);
    await expect(row.getByTestId("triage-model-pending")).toHaveCount(0);
  });
});
