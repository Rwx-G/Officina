// Story 3.9: the evidence on the triage model (FR-10, AD-52, AD-55,
// journey 4) on the registry view the real `officina subjects --json`
// printed after the `registry_fixture` example wrote the `evidence`
// scenario through the window's writer (D-72): confirmations the author
// made from default proposals, a dry run of the disabled model on them,
// and a call refused before it started.
import { expect, test, type Page } from "@playwright/test";
import { calls, loadBoard, loadRegistry, openBoard } from "./fixtures";
import { DRY_RUN_STOP_TEXT, TRIAGE_REFUSAL_TEXT, usd } from "../src/lib/registry-text";
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
    await expect(evidence.getByRole("checkbox")).toHaveCount(0);
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
