// Story 3.9: the evidence on the triage model (FR-10, AD-52, AD-55,
// journey 4) on the registry view the real `officina subjects --json`
// printed after the `registry_fixture` example wrote the `evidence`
// scenario through the window's writer (D-72): confirmations the author
// made from default proposals, a dry run of the disabled model on them,
// and a call refused before it started.
import { expect, test, type Page } from "@playwright/test";
import { calls, loadBoard, loadRegistry, openBoard } from "./fixtures";
import {
  DRY_RUN_STOP_TEXT,
  GATE_STATE_TEXT,
  PROPOSAL_SOURCE_TEXT,
  TRIAGE_REFUSAL_TEXT,
  usd,
} from "../src/lib/registry-text";
import type { RegistryView } from "../src/lib/generated/RegistryView";

async function openEvidence(page: Page, view: RegistryView = loadRegistry("evidence")): Promise<void> {
  await openBoard(page, loadBoard("evidence-board"), {}, null, view);
  await page.getByTestId("view-subjects").click();
  await expect(page.getByTestId("triage-evidence")).toBeVisible();
}

function latestOf(view: RegistryView) {
  const latest = view.model.latest_dry_run;
  if (!latest) throw new Error("no dry run in the evidence fixture: rebuild it with pnpm e2e:fixtures");
  return latest;
}

test.describe("Triage evidence", () => {
  test("a dry run shows its cases, correct counts, false finished, unanswered and cost", async ({ page }) => {
    const view = loadRegistry("evidence");
    const { tally } = latestOf(view);
    await openEvidence(page, view);
    const evidence = page.getByTestId("triage-evidence");
    await expect(evidence.getByTestId("dry-run-cases")).toHaveText(String(tally.cases));
    await expect(evidence.getByTestId("dry-run-model-correct")).toHaveText(String(tally.model_correct));
    await expect(evidence.getByTestId("dry-run-default-correct")).toHaveText(String(tally.default_correct));
    await expect(evidence.getByTestId("dry-run-false-finished")).toHaveText(String(tally.model_false_finished));
    await expect(evidence.getByTestId("dry-run-unanswered")).toHaveText(String(tally.unanswered));
    await expect(evidence.getByTestId("dry-run-cost")).toHaveText(usd(tally.cost_microusd));
    expect(tally.unanswered).toBeGreaterThan(0);
    expect(tally.model_false_finished).toBeGreaterThan(0);
    // The model stays off: no toggle before story 3.11, and no row is a
    // model proposal.
    await expect(evidence.getByTestId("model-state")).toContainText("The model is off");
    await expect(evidence.getByTestId("model-toggle")).toHaveCount(0);
    expect(view.triage.every((row) => row.proposal.source !== "model")).toBe(true);
  });

  test("a running dry run shows its progress", async ({ page }) => {
    const view = loadRegistry("evidence");
    const running: RegistryView = {
      ...view,
      model: { ...view.model, dry_run: { done: 2, total: 6, started_at_ms: view.generated_at_ms } },
    };
    await openEvidence(page, running);
    await expect(page.getByTestId("dry-run-progress")).toHaveText("Dry run: 2 of 6 cases");
    await expect(page.getByTestId("evaluate")).toHaveCount(0);
    // When it ends, the next view shows the result and how it stopped.
    await page.evaluate(
      (done) => window.__officinaPushRegistry?.(done),
      { ...view, model: { ...view.model, dry_run: null, last_stop: "budget_spent" as const } },
    );
    await expect(page.getByTestId("dry-run-progress")).toHaveCount(0);
    await expect(page.getByTestId("dry-run-stop")).toHaveText(DRY_RUN_STOP_TEXT.budget_spent);
    await expect(page.getByTestId("evaluate")).toBeVisible();
  });

  test("evaluate sends evaluate_triage_model", async ({ page }) => {
    await openEvidence(page);
    await page.getByTestId("evaluate").click();
    await expect.poll(async () => (await calls(page)).filter((c) => c.cmd === "evaluate_triage_model").length).toBe(1);
    await expect(page.getByTestId("evaluate-error")).toHaveCount(0);
    // A refusal is shown beside the button, and nothing else changes.
    await page.evaluate(() => {
      window.__officinaEvaluateRefused = "a dry run is already running";
    });
    await page.getByTestId("evaluate").click();
    await expect(page.getByTestId("evaluate-error")).toHaveText("a dry run is already running");
  });

  test("call counts and refusals are shown", async ({ page }) => {
    const view = loadRegistry("evidence");
    const { today, last_30_days: month, refusals } = view.model.calls;
    await openEvidence(page, view);
    const counts = page.getByTestId("call-counts");
    await expect(counts).toContainText(`Today ${today.calls} calls`);
    await expect(counts).toContainText(usd(today.cost_microusd));
    await expect(counts).toContainText(`last 30 days ${month.calls} calls`);
    expect(refusals.length).toBeGreaterThan(0);
    const shown = page.getByTestId("refusal");
    await expect(shown).toHaveCount(refusals.length);
    for (const [i, refusal] of refusals.entries()) {
      await expect(shown.nth(i)).toHaveAttribute("data-cause", refusal.cause);
      await expect(shown.nth(i)).toContainText(TRIAGE_REFUSAL_TEXT[refusal.cause]);
    }
  });
});

// Story 3.10: the measured error and the auto-archive gate (FR-11, FR-12,
// AD-50, AD-55) on the same `evidence` fixture: a nightly job's state
// proposals confirmed, three of its runs auto-archived, the last reopened
// since, the gate that then broke and turned the switch off.
test.describe("Measured error and auto-archive", () => {
  test("per source, flipped finished proposals with the count they rest on", async ({ page }) => {
    const view = loadRegistry("evidence");
    await openEvidence(page, view);
    const shown = page.getByTestId("source-rate");
    await expect(shown).toHaveCount(view.rates.length);
    for (const rate of view.rates.filter((r) => r.finished !== null)) {
      const line = page.locator(`[data-testid="source-rate"][data-source="${rate.source}"]`);
      await expect(line).toContainText(PROPOSAL_SOURCE_TEXT[rate.source]);
      await expect(line.getByTestId("rate-finished")).toHaveText(
        `${rate.finished!.flipped} of its last ${rate.finished!.cases} finished proposals flipped`,
      );
      await expect(line.getByTestId("rate-all")).toHaveText(
        `${rate.all.flipped} flipped among its last ${rate.all.cases} proposals`,
      );
    }
    const state = view.rates.find((r) => r.source === "state")!;
    expect(state.finished).toEqual({ cases: 50, flipped: 1 });
  });

  test("a source with no finished proposal says so", async ({ page }) => {
    const view = loadRegistry("evidence");
    const silent = view.rates.filter((r) => r.finished === null);
    expect(silent.length).toBeGreaterThan(0);
    await openEvidence(page, view);
    for (const rate of silent) {
      await expect(
        page.locator(`[data-testid="source-rate"][data-source="${rate.source}"]`).getByTestId("rate-finished"),
      ).toHaveText("no finished proposal yet");
    }
  });

  test("the auto-archive toggle explains why it is disabled", async ({ page }) => {
    const view = loadRegistry("evidence");
    expect(view.gates.auto_archive.state).not.toBe("holds");
    await openEvidence(page, view);
    await expect(page.getByTestId("auto-archive")).toBeDisabled();
    await expect(page.getByTestId("auto-archive")).not.toBeChecked();
    const reason = page.getByTestId("auto-archive-reason");
    await expect(reason).toContainText(GATE_STATE_TEXT[view.gates.auto_archive.state]);
    // The fixture's switch was turned off by its broken gate: it says so.
    expect(view.auto_archive.src).toBe("rules");
    await expect(reason).toContainText(`It turned itself off: ${GATE_STATE_TEXT[view.auto_archive.reason!]}`);
  });

  test("the toggle sends set_auto_archive while the gate holds", async ({ page }) => {
    const view = loadRegistry("evidence");
    const holding: RegistryView = {
      ...view,
      gates: { ...view.gates, auto_archive: { ...view.gates.auto_archive, state: "holds" } },
      auto_archive: { on: false, src: null, at_ms: null, reason: null },
    };
    await openEvidence(page, holding);
    const toggle = page.getByTestId("auto-archive");
    await expect(toggle).toBeEnabled();
    await toggle.click();
    await expect.poll(async () => (await calls(page)).filter((c) => c.cmd === "set_auto_archive").map((c) => c.args)).toEqual([
      { on: true },
    ]);
    await expect(toggle).toBeChecked();
    await toggle.click();
    await expect
      .poll(async () => (await calls(page)).filter((c) => c.cmd === "set_auto_archive").length)
      .toBe(2);
    await expect(toggle).not.toBeChecked();
  });

  test("a refused toggle stays as the registry holds it and says why", async ({ page }) => {
    const view = loadRegistry("evidence");
    const holding: RegistryView = {
      ...view,
      gates: { ...view.gates, auto_archive: { ...view.gates.auto_archive, state: "holds" } },
      auto_archive: { on: false, src: null, at_ms: null, reason: null },
    };
    await openEvidence(page, holding);
    // The gate broke in the backend after the window showed it holding.
    await page.evaluate((broken) => {
      window.__officinaRegistry = broken;
    }, view);
    await page.getByTestId("auto-archive").click();
    await expect(page.getByTestId("auto-archive-error")).toContainText("its gate does not hold");
    await expect(page.getByTestId("auto-archive")).not.toBeChecked();
  });

  test("recent auto-archives can be reopened", async ({ page }) => {
    const view = loadRegistry("evidence");
    const archived = view.recent_auto_archives.find((a) => a.archived);
    const reopened = view.recent_auto_archives.find((a) => a.reopened_at_ms !== null);
    if (!archived || !reopened) throw new Error("the evidence fixture lacks recent auto-archives: rebuild it");
    await openEvidence(page, view);
    await expect(page.getByTestId("auto-archived")).toHaveCount(view.recent_auto_archives.length);
    const row = page.locator(`[data-testid="auto-archived"][data-subject="${archived.subject.id}"]`);
    await expect(row).toContainText(archived.subject.name);
    await row.getByTestId("auto-archive-reopen").click();
    await expect
      .poll(async () => (await calls(page)).filter((c) => c.cmd === "reopen_subject").map((c) => c.args))
      .toEqual([{ subject: archived.subject.id }]);
    const again = page.locator(`[data-testid="auto-archived"][data-subject="${reopened.subject.id}"]`);
    await expect(again.getByTestId("auto-archive-reopened")).toBeVisible();
    await expect(again.getByTestId("auto-archive-reopen")).toHaveCount(0);
  });
});
