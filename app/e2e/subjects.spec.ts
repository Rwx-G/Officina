// Story 3.5: the Subjects view (FR-6, AD-55) on registry views printed by
// the real `officina subjects --json` after the `registry_fixture` example
// wrote the registry through the window's writer (D-72): `subjects` holds
// an active subject with a live session, an active one whose session ended
// with its excerpts, a `to_resume` one, an archived and a merged one, a
// subject whose name and prompt are markup, unfiled sessions and twelve
// non-interactive runs; `subjects-synced`, `subjects-restored` and
// `subjects-unavailable` carry the registry notes.
import { expect, test, type Page } from "@playwright/test";
import { calls, loadBoard, loadRegistry, openBoard } from "./fixtures";
import { AUTHOR_REFUSAL_TEXT, LIVE_STATE_TEXT, REGISTRY_STATE_TEXT, SUBJECT_STATUS_TEXT } from "../src/lib/registry-text";
import type { RegistryView } from "../src/lib/generated/RegistryView";
import type { SubjectView } from "../src/lib/generated/SubjectView";

async function openSubjects(page: Page, registry: RegistryView = loadRegistry("subjects")): Promise<void> {
  await openBoard(page, loadBoard("subjects-board"), {}, null, registry);
  await page.getByTestId("view-subjects").click();
  await expect(page.getByTestId("subjects-view")).toBeVisible();
}

function subjectNamed(view: RegistryView, name: string): SubjectView {
  const found = view.subjects.find((s) => s.name === name);
  if (!found) throw new Error(`no subject ${name} in the fixture: rebuild it with pnpm e2e:fixtures`);
  return found;
}

function card(page: Page, subject: SubjectView) {
  return page.locator(`[data-testid="subject-card"][data-subject="${subject.id}"]`);
}

function row(page: Page, sessionId: string) {
  return page.locator(`[data-testid="session-row"][data-session="${sessionId}"]`);
}

async function authorCalls(page: Page): Promise<{ cmd: string; args: unknown }[]> {
  const author = ["create_subject", "rename_subject", "merge_subjects", "file_sessions", "archive_subject", "reopen_subject"];
  return (await calls(page)).filter((c) => author.includes(c.cmd));
}

test.describe("header", () => {
  test("the header switches between Board and Subjects", async ({ page }) => {
    await openBoard(page, loadBoard("subjects-board"), {}, null, loadRegistry("subjects"));
    const board = page.getByTestId("view-board");
    const subjects = page.getByTestId("view-subjects");
    await expect(board).toHaveAttribute("aria-pressed", "true");
    await subjects.click();
    await expect(page.getByTestId("subjects-view")).toBeVisible();
    await expect(page.getByTestId("column-needs_you")).toHaveCount(0);
    await expect(subjects).toHaveAttribute("aria-pressed", "true");
    await board.click();
    await expect(page.getByTestId("column-needs_you")).toBeVisible();
    await expect(page.getByTestId("subjects-view")).toHaveCount(0);
  });

  test("number keys do nothing in the Subjects view", async ({ page }) => {
    const board = loadBoard("busy");
    await openBoard(page, board, {}, null, loadRegistry("subjects"));
    await page.getByTestId("view-subjects").click();
    await expect(page.getByTestId("subjects-view")).toBeVisible();
    await page.keyboard.press("1");
    expect((await calls(page)).filter((c) => c.cmd === "focus")).toHaveLength(0);
    // Positive control: the same key focuses in the Board view.
    await page.getByTestId("view-board").click();
    await page.keyboard.press("1");
    expect((await calls(page)).filter((c) => c.cmd === "focus")).toEqual([
      { cmd: "focus", args: { key: board.columns.needs_you[0]!.key } },
    ]);
  });
});

test.describe("Subjects view", () => {
  test("active and to-resume subjects come first with their sessions' live state", async ({ page }) => {
    const view = loadRegistry("subjects");
    await openSubjects(page, view);
    const cards = page.getByTestId("subject-card");
    await expect(cards).toHaveCount(view.subjects.length);
    for (const [i, subject] of view.subjects.entries()) {
      await expect(cards.nth(i)).toHaveAttribute("data-subject", String(subject.id));
      await expect(cards.nth(i).getByTestId("subject-status")).toHaveText(SUBJECT_STATUS_TEXT[subject.status]);
    }
    const statuses = view.subjects.map((s) => s.status);
    expect(statuses.lastIndexOf("active")).toBeLessThan(statuses.indexOf("to_resume"));
    const live = view.subjects.flatMap((s) => s.sessions).filter((s) => s.card && !s.ended);
    expect(live.length).toBeGreaterThan(0);
    for (const session of live) {
      await expect(row(page, session.session_id).getByTestId("session-state")).toHaveText(
        LIVE_STATE_TEXT[session.card!.column],
      );
    }
  });

  test("a subject without a live session shows its last prompt and the end of its last answer", async ({ page }) => {
    const view = loadRegistry("subjects");
    await openSubjects(page, view);
    const quiet = subjectNamed(view, "flaky retry");
    expect(quiet.sessions.every((s) => s.ended !== null)).toBe(true);
    await expect(card(page, quiet).getByTestId("subject-prompt")).toHaveText(quiet.excerpts!.prompt!);
    await expect(card(page, quiet).getByTestId("subject-answer")).toHaveText(quiet.excerpts!.answer_tail!);
    const busy = subjectNamed(view, "invoice export");
    await expect(card(page, busy).getByTestId("subject-excerpts")).toHaveCount(0);
  });

  test("unfiled sessions are listed under their project", async ({ page }) => {
    const view = loadRegistry("subjects");
    await openSubjects(page, view);
    for (const group of view.unfiled) {
      const shown = page.locator(`[data-testid="unfiled-project"][data-project="${group.project}"]`);
      await expect(shown.getByRole("heading")).toHaveText(group.project);
      for (const session of group.sessions) {
        await expect(shown.locator(`[data-session="${session.session_id}"]`)).toBeVisible();
      }
    }
  });

  test("non-interactive runs are counted and collapsed", async ({ page }) => {
    const view = loadRegistry("subjects");
    const group = view.unfiled.find((p) => p.non_interactive_count > p.non_interactive.length)!;
    await openSubjects(page, view);
    const shown = page.locator(`[data-testid="unfiled-project"][data-project="${group.project}"]`);
    const toggle = shown.getByTestId("non-interactive-toggle");
    await expect(toggle).toHaveText(`${group.non_interactive_count} non-interactive runs`);
    await expect(toggle).toHaveAttribute("aria-expanded", "false");
    await expect(shown.getByTestId("non-interactive")).toHaveCount(0);
    await toggle.click();
    await expect(shown.getByTestId("non-interactive").getByTestId("session-row")).toHaveCount(
      group.non_interactive.length,
    );
    await expect(shown).toContainText(`The ${group.non_interactive.length} most recent are listed.`);
  });

  test("archived subjects appear on demand", async ({ page }) => {
    const view = loadRegistry("subjects");
    const archived: SubjectView = { ...subjectNamed(view, "ledger cleanup"), status: "archived", routine: true };
    await page.addInitScript((list) => (window.__officinaArchived = list), [archived]);
    await openSubjects(page, view);
    await expect(page.getByTestId("archived-subject")).toHaveCount(0);
    await page.getByTestId("show-archived").click();
    const item = page.locator(`[data-testid="archived-subject"][data-subject="${archived.id}"]`);
    await expect(item).toContainText(archived.name);
    await expect(item.getByTestId("routine")).toBeVisible();
    expect((await calls(page)).filter((c) => c.cmd === "list_archived")).toHaveLength(1);
    await item.getByRole("button", { name: "Reopen" }).click();
    await expect.poll(() => authorCalls(page)).toEqual([{ cmd: "reopen_subject", args: { subject: archived.id } }]);
  });

  // Story 3.10 AC 7 (FR-13, D-55): the fixture archived "release notes"
  // with the routine box and merged "notes copy" away; only the first is a
  // candidate.
  test("routine subjects are listed as candidates", async ({ page }) => {
    const view = loadRegistry("subjects");
    const candidates = view.routine_candidates;
    expect(candidates.map((c) => c.subject.name)).toEqual(["release notes"]);
    await openSubjects(page, view);
    const shown = page.getByTestId("routine-candidate");
    await expect(shown).toHaveCount(candidates.length);
    for (const [i, candidate] of candidates.entries()) {
      await expect(shown.nth(i)).toHaveAttribute("data-subject", String(candidate.subject.id));
      await expect(shown.nth(i)).toContainText(candidate.subject.name);
      await expect(shown.nth(i)).toContainText(candidate.project);
    }
  });

  test("each action sends its command with the shown arguments", async ({ page }) => {
    const view = loadRegistry("subjects");
    await openSubjects(page, view);
    const quiet = subjectNamed(view, "flaky retry");
    const busy = subjectNamed(view, "invoice export");

    await card(page, quiet).getByRole("button", { name: "Rename" }).click();
    await card(page, quiet).getByTestId("rename-input").fill("flaky retry, payments");
    await card(page, quiet).getByRole("button", { name: "Save" }).click();

    await card(page, quiet).getByRole("button", { name: "Merge" }).click();
    await card(page, quiet).getByTestId("merge-target").selectOption({ label: `${busy.name} (${busy.project})` });
    await card(page, quiet).getByTestId("merge-confirm").click();

    const loose = view.unfiled.flatMap((p) => p.sessions)[0]!;
    await row(page, loose.session_id).getByRole("checkbox").check();
    await page
      .getByTestId("selection-bar")
      .locator(`[data-testid="pick-subject"][data-subject="${busy.id}"]`)
      .click();

    const filed = busy.sessions[0]!;
    await row(page, filed.session_id).getByRole("checkbox").check();
    await page.getByTestId("unfile").click();

    const form = page.getByTestId("new-subject");
    await form.getByLabel("New subject name").fill("search index");
    await form.getByLabel("Project of the new subject").selectOption(view.projects.at(-1)!);
    await form.getByRole("button", { name: "Create subject" }).click();

    await expect.poll(() => authorCalls(page)).toEqual([
      { cmd: "rename_subject", args: { subject: quiet.id, name: "flaky retry, payments" } },
      { cmd: "merge_subjects", args: { source: quiet.id, target: busy.id } },
      { cmd: "file_sessions", args: { sessions: [loose.session_id], subject: busy.id } },
      { cmd: "file_sessions", args: { sessions: [filed.session_id], subject: null } },
      { cmd: "create_subject", args: { name: "search index", origin: { project: view.projects.at(-1)! } } },
    ]);

    // The screen follows the collector's next view, pushed as the event.
    const renamed = {
      ...view,
      subjects: view.subjects.map((s) => (s.id === quiet.id ? { ...s, name: "flaky retry, payments" } : s)),
    };
    await page.evaluate(async (next) => window.__officinaPushRegistry?.(next), renamed);
    await expect(card(page, quiet).getByTestId("subject-name")).toHaveText("flaky retry, payments");
  });

  test("creating from selected sessions takes their project", async ({ page }) => {
    const view = loadRegistry("subjects");
    await openSubjects(page, view);
    const group = view.unfiled.find((p) => p.sessions.length > 0 && view.subjects.some((s) => s.project === p.project))!;
    const session = group.sessions[0]!;
    await row(page, session.session_id).getByRole("checkbox").check();
    const bar = page.getByTestId("selection-bar");
    await expect(bar).toContainText(`1 session selected in ${group.project}`);
    const own = view.subjects.filter((s) => s.project === group.project).map((s) => String(s.id));
    const offered = await bar.getByTestId("pick-subject").evaluateAll((buttons) =>
      buttons.map((b) => b.getAttribute("data-subject")),
    );
    expect(offered.slice(0, own.length)).toEqual(own);
    await bar.getByTestId("picker-new-name").fill("chart upgrade");
    await bar.getByTestId("picker-create").click();
    await expect.poll(() => authorCalls(page)).toEqual([
      { cmd: "create_subject", args: { name: "chart upgrade", origin: { sessions: [session.session_id] } } },
    ]);
  });

  test("archive offers the routine box", async ({ page }) => {
    const view = loadRegistry("subjects");
    await openSubjects(page, view);
    const quiet = subjectNamed(view, "flaky retry");
    await card(page, quiet).getByRole("button", { name: "Archive" }).first().click();
    const box = card(page, quiet).getByTestId("routine-box");
    await expect(box).not.toBeChecked();
    await expect(card(page, quiet)).toContainText("Routine, could have run alone");
    await box.check();
    await card(page, quiet).getByTestId("archive-confirm").click();
    await expect
      .poll(() => authorCalls(page))
      .toEqual([{ cmd: "archive_subject", args: { subject: quiet.id, routine: true } }]);
  });

  test("a refused action shows the reason and changes nothing", async ({ page }) => {
    const view = loadRegistry("subjects");
    await openSubjects(page, view);
    await page.evaluate(() => (window.__officinaRefuse = { rename_subject: "name_taken" }));
    const quiet = subjectNamed(view, "flaky retry");
    const before = await page.getByTestId("subjects-view").getByTestId("subject-name").allTextContents();
    await card(page, quiet).getByRole("button", { name: "Rename" }).click();
    await card(page, quiet).getByTestId("rename-input").fill("invoice export");
    await card(page, quiet).getByRole("button", { name: "Save" }).click();
    await expect(card(page, quiet).getByTestId("action-error")).toHaveText(AUTHOR_REFUSAL_TEXT.name_taken);
    await expect(page.getByTestId("subjects-view").getByTestId("subject-name")).toHaveText(before);
    await expect(card(page, quiet).getByTestId("rename-input")).toBeVisible();
  });

  test("a synced registry names the folder to exclude", async ({ page }) => {
    const view = loadRegistry("subjects-synced");
    await openSubjects(page, view);
    await expect(page.getByTestId("registry-synced")).toContainText(view.registry.exclude_from_sync!);
    await openSubjects(page, loadRegistry("subjects"));
    await expect(page.getByTestId("registry-synced")).toHaveCount(0);
  });

  test("a restored registry names the copy used", async ({ page }) => {
    const view = loadRegistry("subjects-restored");
    await openSubjects(page, view);
    await expect(page.getByTestId("registry-restored")).toContainText(view.restore_notice!.copy);
    await openSubjects(page, loadRegistry("subjects"));
    await expect(page.getByTestId("registry-restored")).toHaveCount(0);
  });

  test("an unavailable registry says why", async ({ page }) => {
    const view = loadRegistry("subjects-unavailable");
    await openSubjects(page, view);
    const banner = page.getByTestId("registry-unavailable");
    await expect(banner).toContainText(REGISTRY_STATE_TEXT.unavailable!);
    await expect(banner).toContainText(view.registry.detail!);
    await expect(page.getByTestId("unfiled")).toBeVisible();
    await expect(page.getByTestId("show-archived")).toBeVisible();
  });

  test("session and subject text renders as text, never as HTML", async ({ page }) => {
    const view = loadRegistry("subjects");
    const hostile = view.subjects.find((s) => s.name.includes("<img"))!;
    expect(hostile).toBeTruthy();
    await openSubjects(page, view);
    expect(hostile.excerpts?.prompt).toContain("<img");
    const shown = page.getByTestId("subjects-view");
    await expect(card(page, hostile).getByTestId("subject-name")).toHaveText(hostile.name);
    await expect(card(page, hostile).getByTestId("subject-prompt")).toHaveText(hostile.excerpts!.prompt!);
    await page.getByTestId("unfiled").getByRole("checkbox").first().check();
    await expect(page.getByTestId("selection-bar")).toContainText(hostile.name);
    await expect(page.locator("img:not([alt=''])")).toHaveCount(0);
    await expect(shown.locator("img")).toHaveCount(0);
    expect(await page.evaluate(() => (window as { __officinaXss?: number }).__officinaXss)).toBeUndefined();
  });
});
