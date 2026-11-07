// Story 6.19: the workshop card's facts, the review panel and the inert
// rendering of every run text (AD-122, FR-6, FR-10 to FR-12, D-245).
// Story 6.20: the Workshop section's actions (AD-122, FR-17, D-28, D-607).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import { tick } from "svelte";
import WorkOrderCard from "./WorkOrderCard.svelte";
import ReviewPanel from "./ReviewPanel.svelte";
import WorkshopCard from "./WorkshopCard.svelte";
import WorkshopSection from "./WorkshopSection.svelte";
import Header from "./Header.svelte";
import { BoardStore } from "../lib/board.svelte";
import * as ipc from "../lib/ipc";
import { workshop } from "../lib/workshop.svelte";
import { FORMAT_RANGES, escapeFormat, escapeOf } from "../lib/inert";
import { AUTHOR_DISCARD_REASONS, CREDENTIAL_STATE_TEXT } from "../lib/workshop-text";
import { fixtureOf } from "../../e2e/fixture-index";
import {
  UNTRUSTED_SOURCES,
  approvedReview,
  noChangeReview,
  quarantinedReview,
  review,
  workshopCard,
} from "../../e2e/workshop-fixtures";
import injection from "../../../tests/vectors/injection.json";
import type { Board } from "../lib/generated/Board";
import type { Card } from "../lib/generated/Card";
import type { CredentialAction } from "../lib/generated/CredentialAction";
import type { CredentialKind } from "../lib/generated/CredentialKind";
import type { CredentialState } from "../lib/generated/CredentialState";
import type { ReviewView } from "../lib/generated/ReviewView";
import type { WorkOrderCard as WorkOrderCardPayload } from "../lib/generated/WorkOrderCard";
import type { WorkshopCardFacts } from "../lib/generated/WorkshopCardFacts";
import type { WorkshopView } from "../lib/generated/WorkshopView";

vi.mock("../lib/ipc", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../lib/ipc")>()),
  openReview: vi.fn(async (runId: string) => ({ ...review(), run_id: runId })),
  approveSensitive: vi.fn(async () => ({ outcome: "approved" })),
  importPackage: vi.fn(async () => ({ outcome: "imported", detail: "officina/dependency-bump-5" })),
  discardPackage: vi.fn(async () => ({ outcome: "discarded" })),
  runBattery: vi.fn(async () => ({ outcome: "queued", detail: ["officina-workshop:local"] })),
  buildImages: vi.fn(async () => ({ outcome: "queued" })),
  stopWorkshop: vi.fn(async () => ({ containers: 2, runs: 1, engine_reached: true, elapsed_ms: 1_200 })),
  resumeWorkshop: vi.fn(async () => ({ collected: 1, removed: 0, relay_resumed: true })),
  removeRun: vi.fn(async () => ({ status: "removed", detail: 3 })),
  prepareLogin: vi.fn(async () => ({ status: "ready", detail: "docker run --rm -it officina-workshop:local claude /login" })),
  checkLogin: vi.fn(async () => ({
    status: "checked",
    detail: { logged_in: true, auth_method: "claude.ai", unreadable: false, state: "unchecked" },
  })),
}));

const NOW = 1_800_000_000_000;

interface KeyboardStep {
  launch?: { detail?: number };
  import?: { detail?: number };
}
interface InjectionCase {
  name: string;
  tags?: string[];
  steps: KeyboardStep[];
}
const keyboardCases = (section: "workshop_card" | "workshop_import"): InjectionCase[] =>
  (injection[section] as InjectionCase[]).filter((c) => c.tags?.includes("keyboard"));

const base = (): WorkOrderCardPayload => fixtureOf<WorkOrderCardPayload>("card", "card-bump");

function renderCard(facts: Partial<WorkshopCardFacts> = {}) {
  const onLaunch = vi.fn();
  const onBase = vi.fn();
  const rendered = render(WorkOrderCard, {
    card: workshopCard(base(), facts),
    order: null,
    launching: false,
    launch: null,
    nowMs: NOW,
    busy: false,
    onLaunch,
    onPermission: vi.fn(),
    onBase,
  });
  return { ...rendered, onLaunch, onBase };
}

async function openPanel(view: ReviewView) {
  vi.mocked(ipc.openReview).mockResolvedValue(view);
  await workshop.open(view.run_id);
  const rendered = render(ReviewPanel, { nowMs: NOW });
  await tick();
  return rendered;
}

beforeEach(() => {
  vi.clearAllMocks();
  workshop.close();
  workshop.view = null;
  workshop.battery = null;
  workshop.build = null;
  workshop.stopped = null;
  workshop.resumed = null;
  workshop.login = null;
  workshop.loginCheck = null;
  workshop.removed = {};
});
afterEach(cleanup);

describe("the workshop card", () => {
  it("the base select offers the closed list only", async () => {
    const { getByTestId, getAllByTestId, container, onBase } = renderCard();
    const select = getByTestId("workshop-base") as HTMLSelectElement;
    const facts = workshopCard(base()).workshop!;
    expect(getAllByTestId("workshop-base-option")).toHaveLength(facts.bases.length);
    expect(container.querySelector("[data-testid=workshop-facts] input")).toBeNull();
    expect(getAllByTestId("workshop-base-option")[2]!.textContent).toContain("from run");
    select.value = "1";
    await fireEvent.change(select);
    expect(onBase).toHaveBeenCalledWith(facts.bases[1]!.choice);
    expect(getByTestId("workshop-uncommitted").textContent).toContain("not carried");
    expect(getByTestId("workshop-mount").textContent).toContain("read-only");
    expect(getByTestId("workshop-image").textContent).toContain("0123456789ab");
    expect(getByTestId("workshop-chip").textContent).toBe("workshop");
  });

  it("launch is disabled while a cause holds", async () => {
    const { getByTestId, getAllByTestId, onLaunch } = renderCard({
      readiness: [{ refusal: "engine_unavailable", detail: null }],
    });
    const launch = getByTestId("launch") as HTMLButtonElement;
    expect(launch.disabled).toBe(true);
    await fireEvent.click(launch, { detail: 1 });
    expect(onLaunch).not.toHaveBeenCalled();
    expect(getAllByTestId("workshop-cause").map((c) => c.dataset.cause)).toEqual(["engine_unavailable"]);
  });

  it("launch ignores detail 0", async () => {
    const { getByTestId, onLaunch } = renderCard();
    const launch = getByTestId("launch");
    await fireEvent.click(launch, { detail: 0 });
    expect(onLaunch).not.toHaveBeenCalled();
    await fireEvent.click(launch, { detail: 1 });
    expect(onLaunch).toHaveBeenCalledTimes(1);
  });

  it("a stale battery offers the battery", async () => {
    const { getByTestId, findByTestId } = renderCard({ readiness: [{ refusal: "battery_stale", detail: null }] });
    const battery = getByTestId("workshop-battery");
    await fireEvent.click(battery, { detail: 0 });
    expect(ipc.runBattery).not.toHaveBeenCalled();
    await fireEvent.click(battery, { detail: 1 });
    expect(ipc.runBattery).toHaveBeenCalledWith("officina-workshop:local");
    await findByTestId("workshop-battery-queued");
  });

  it("an expired credential shows its action for each kind", async () => {
    const writeText = vi.fn(async () => undefined);
    Object.assign(navigator, { clipboard: { writeText } });
    const view = fixtureOf<WorkshopView>("workshop", "workshop-runs");
    const hostile = `<img src=x onerror="window.__hit = 1">`;
    const actions: CredentialAction[] = [
      { kind: "login", command: `docker run --rm -it officina-workshop:local claude /login ${hostile}` },
      { kind: "setup_token", step: `Run claude setup-token ${hostile}`, cmdkey_line: `cmdkey /generic:officina /user:user01 ${hostile}` },
    ];
    for (const action of actions) {
      const login = action.kind === "login";
      workshop.view = {
        ...view,
        credential: { kind: login ? "login" : "oauth-token", state: login ? "needs_login" : "needs_new_credential", action },
      };
      const { getByTestId, container } = renderCard({ readiness: [{ refusal: "credential_expired", detail: null }] });
      const shown = getByTestId("workshop-credential-action");
      expect(shown.dataset.kind).toBe(action.kind);
      expect(container.querySelector("img")).toBeNull();
      const copied = action.kind === "login" ? action.command : action.cmdkey_line;
      expect(shown.textContent).toContain(copied);
      await fireEvent.click(getByTestId("workshop-copy"), { detail: 0 });
      expect(writeText).toHaveBeenLastCalledWith(copied);
      cleanup();
    }
  });

  it("injection keyboard cases never call launch", async () => {
    const cases = keyboardCases("workshop_card");
    expect(cases.length).toBeGreaterThan(0);
    for (const vector of cases) {
      const { getByTestId, onLaunch } = renderCard();
      const launch = getByTestId("launch");
      for (const step of vector.steps) {
        if (!step.launch) continue;
        await fireEvent.keyDown(launch, { key: "Enter" });
        await fireEvent.keyDown(launch, { key: " " });
        await fireEvent.click(launch, { detail: step.launch.detail ?? 0 });
      }
      expect(onLaunch, vector.name).not.toHaveBeenCalled();
      cleanup();
    }
  });
});

describe("the board card", () => {
  const cards = (): Card[] =>
    Object.values(fixtureOf<Board>("workshop", "workshop-runs-board").columns)
      .flat()
      .filter((c: Card) => c.kind === "workshop");

  it("receive is offered on Needs you and Done only, and the isolated chip on Working only", () => {
    for (const card of cards()) {
      const { queryByTestId } = render(WorkshopCard, { card, nowMs: NOW });
      expect(queryByTestId("receive") !== null, card.key).toBe(card.column === "needs_you" || card.column === "done");
      expect(queryByTestId("isolated-chip") !== null, card.key).toBe(card.column === "working");
      cleanup();
    }
  });
});

describe("the review panel", () => {
  it("approve and import ignore detail 0", async () => {
    const { getByTestId } = await openPanel(review());
    const approve = getByTestId("approve-group");
    await fireEvent.click(approve, { detail: 0 });
    await fireEvent.keyDown(approve, { key: "Enter" });
    expect(ipc.approveSensitive).not.toHaveBeenCalled();
    await fireEvent.click(approve, { detail: 1 });
    expect(ipc.approveSensitive).toHaveBeenCalledWith(review().run_id, "ci", "3".repeat(64));
    cleanup();

    const ready = await openPanel(approvedReview());
    const importButton = ready.getByTestId("review-import") as HTMLButtonElement;
    expect(importButton.disabled).toBe(false);
    await fireEvent.click(importButton, { detail: 0 });
    expect(ipc.importPackage).not.toHaveBeenCalled();
    await fireEvent.click(importButton, { detail: 1 });
    expect(ipc.importPackage).toHaveBeenCalledWith(review().run_id, review().tip);
  });

  it("import stays disabled until every group is approved", async () => {
    const { getByTestId } = await openPanel(review());
    expect((getByTestId("review-import") as HTMLButtonElement).disabled).toBe(true);
    expect(getByTestId("review-not-ready").textContent).toContain("Approve each sensitive group");
  });

  it("injection keyboard import cases never call import", async () => {
    const cases = keyboardCases("workshop_import");
    expect(cases.length).toBeGreaterThan(0);
    for (const vector of cases) {
      const { getByTestId } = await openPanel(approvedReview());
      const importButton = getByTestId("review-import");
      for (const step of vector.steps) {
        if (!step.import) continue;
        await fireEvent.keyDown(importButton, { key: "Enter" });
        await fireEvent.click(importButton, { detail: step.import.detail ?? 0 });
      }
      expect(ipc.importPackage, vector.name).not.toHaveBeenCalled();
      cleanup();
      workshop.close();
    }
  });

  it("discard is keyboard reachable", async () => {
    const { getAllByTestId } = await openPanel(review());
    const reasons = getAllByTestId("review-discard");
    expect(reasons.map((b) => b.dataset.reason)).toEqual([...AUTHOR_DISCARD_REASONS]);
    expect(reasons.every((b) => b.tabIndex === 0)).toBe(true);
    await fireEvent.click(reasons[1]!, { detail: 0 });
    expect(ipc.discardPackage).toHaveBeenCalledWith(review().run_id, "poor_result");
  });

  it("a quarantined package offers discard only", async () => {
    const { getByTestId, queryByTestId, queryAllByTestId } = await openPanel(quarantinedReview());
    expect(getByTestId("review-quarantine").dataset.cause).toBe("credential_found");
    expect(getByTestId("review-revoke").textContent).toContain("Revoke");
    expect(queryByTestId("review-import")).toBeNull();
    expect(queryAllByTestId("review-discard")).toHaveLength(AUTHOR_DISCARD_REASONS.length);
  });

  it("no change shows nothing to review and no action", async () => {
    const view = noChangeReview();
    const { getByTestId, queryByTestId, queryAllByTestId, container } = await openPanel(view);
    const nothing = getByTestId("review-nothing");
    expect(nothing.textContent).toContain(`no commit past main @ ${view.base_commit.slice(0, 12)}`);
    expect(getByTestId("review-report").textContent).toBe(view.report_tail);
    // Above its facts and report.
    expect(nothing.compareDocumentPosition(getByTestId("review-facts")) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(queryByTestId("review-import")).toBeNull();
    expect(queryAllByTestId("review-discard")).toHaveLength(0);
    expect(container.querySelectorAll("button")).toHaveLength(1);
  });
});

describe("inert text", () => {
  it("review texts are inert", async () => {
    const sources = Object.entries(UNTRUSTED_SOURCES);
    expect(sources.length).toBeGreaterThan(0);
    for (const [source, put] of sources) {
      const marker = `<img data-source="${source}" src=x onerror="window.__hit = 1"><b>${source}</b>`;
      const { container } = await openPanel(put(review(), marker));
      expect(container.querySelector("img"), source).toBeNull();
      expect(container.querySelector("b"), source).toBeNull();
      expect(container.textContent, source).toContain(marker);
      cleanup();
      workshop.close();
    }
  });

  it("every format range renders as an escape", async () => {
    expect(FORMAT_RANGES.length).toBeGreaterThan(0);
    for (const [first, last] of FORMAT_RANGES) {
      for (const codePoint of new Set([first, last])) {
        const raw = String.fromCodePoint(codePoint);
        expect(escapeFormat(`a${raw}b`)).toBe(`a${escapeOf(codePoint)}b`);
        for (const [source, put] of Object.entries(UNTRUSTED_SOURCES)) {
          const { container } = await openPanel(put(review(), `x${raw}y`));
          const text = container.textContent ?? "";
          expect(text.includes(raw), `${source} U+${codePoint.toString(16)}`).toBe(false);
          expect(text, source).toContain(`x${escapeOf(codePoint)}y`);
          cleanup();
          workshop.close();
        }
      }
    }
  });

  it("the approval hash stays the raw bytes'", async () => {
    const raw = review();
    const seeded = UNTRUSTED_SOURCES.hunks!(raw, "+‮evil\n");
    const { getByTestId } = await openPanel(seeded);
    expect(getByTestId("group-hunks").textContent).toContain(escapeOf(0x202e));
    expect(getByTestId("group-sha").textContent).toContain(seeded.groups[0]!.hunks_sha256!);
  });
});

// The Workshop section on the real `workshop-runs` view (story 6.18's
// fixture: four live runs, one kept, a token floor whose credential is
// missing), with the fields a case needs changed.
const runsView = (): WorkshopView => fixtureOf<WorkshopView>("workshop", "workshop-runs");

function renderSection(view: WorkshopView) {
  workshop.view = view;
  return render(WorkshopSection, { view, nowMs: NOW });
}

const READY_IMAGE = {
  tag: "officina-workshop:local",
  kind: "workshop",
  state: "ready",
  mismatch: null,
  label: null,
  image_id: "sha256:0",
  inventories: [],
  battery: { outcome: "passed", finished_at_ms: NOW - 60_000, valid: false },
} as const satisfies WorkshopView["images"][number];

function credentialOf(kind: CredentialKind, state: CredentialState): WorkshopView["credential"] {
  const gives = state === "missing" || state === "needs_login" || state === "needs_new_credential";
  const action: CredentialAction | null = !gives
    ? null
    : kind === "login"
      ? { kind: "login", command: "docker run --rm -it officina-workshop:local claude /login" }
      : { kind: "setup_token", step: "claude setup-token", cmdkey_line: "cmdkey /generic:officina /user:workshop /pass" };
  return { kind, state, action };
}

describe("the Workshop section", () => {
  it("build images ignores detail 0", async () => {
    const { getByTestId, findByTestId } = renderSection({ ...runsView(), images: [READY_IMAGE] });
    const build = getByTestId("build-images");
    await fireEvent.keyDown(build, { key: "Enter" });
    await fireEvent.click(build, { detail: 0 });
    expect(ipc.buildImages).not.toHaveBeenCalled();
    await fireEvent.click(build, { detail: 1 });
    expect(ipc.buildImages).toHaveBeenCalledTimes(1);
    await findByTestId("build-queued");
  });

  it("run escape battery ignores detail 0", async () => {
    const { getByTestId, findByTestId } = renderSection({ ...runsView(), images: [READY_IMAGE] });
    expect(getByTestId("battery-row").dataset.validity).toBe("stale");
    const run = getByTestId("run-battery");
    await fireEvent.click(run, { detail: 0 });
    expect(ipc.runBattery).not.toHaveBeenCalled();
    await fireEvent.click(run, { detail: 1 });
    expect(ipc.runBattery).toHaveBeenCalledWith(null);
    await findByTestId("battery-queued");
  });

  it("resume ignores detail 0", async () => {
    const { getByTestId } = renderSection({ ...runsView(), workshop_stopped: true });
    await fireEvent.click(getByTestId("resume"), { detail: 0 });
    const confirm = getByTestId("resume-confirm");
    await fireEvent.keyDown(confirm, { key: "Enter" });
    await fireEvent.click(confirm, { detail: 0 });
    expect(ipc.resumeWorkshop).not.toHaveBeenCalled();
    await fireEvent.click(confirm, { detail: 1 });
    expect(ipc.resumeWorkshop).toHaveBeenCalledTimes(1);
  });

  it("resume defaults to collect and to resuming the relay", async () => {
    const view = { ...runsView(), workshop_stopped: true };
    expect(view.runs.kept.length).toBeGreaterThan(0);
    const { getByTestId, getAllByTestId, queryByTestId, findByTestId } = renderSection(view);
    await fireEvent.click(getByTestId("resume"), { detail: 1 });
    expect(getAllByTestId("resume-collect").every((r) => (r as HTMLInputElement).checked)).toBe(true);
    expect((getByTestId("resume-relay") as HTMLInputElement).checked).toBe(true);
    await fireEvent.click(getByTestId("resume-confirm"), { detail: 1 });
    expect(ipc.resumeWorkshop).toHaveBeenCalledWith(
      view.runs.kept.map((run) => ({ run_id: run.id, choice: "collect" })),
      true,
    );
    await findByTestId("resume-answer");
    expect(queryByTestId("resume-dialog")).toBeNull();
  });

  it("log in and check login ignore detail 0", async () => {
    const { getByTestId, findByTestId } = renderSection({ ...runsView(), credential: credentialOf("login", "unchecked") });
    for (const id of ["log-in", "check-login"]) {
      await fireEvent.keyDown(getByTestId(id), { key: " " });
      await fireEvent.click(getByTestId(id), { detail: 0 });
    }
    expect(ipc.prepareLogin).not.toHaveBeenCalled();
    expect(ipc.checkLogin).not.toHaveBeenCalled();
    await fireEvent.click(getByTestId("log-in"), { detail: 1 });
    await fireEvent.click(getByTestId("check-login"), { detail: 1 });
    expect(ipc.prepareLogin).toHaveBeenCalledTimes(1);
    expect(ipc.checkLogin).toHaveBeenCalledTimes(1);
    expect((await findByTestId("login-check")).textContent).toContain("claude.ai");
  });

  it("the login command renders as text with a copy button", async () => {
    const writeText = vi.fn(async () => undefined);
    Object.assign(navigator, { clipboard: { writeText } });
    const hostile = `docker run --rm -it officina-workshop:local claude /login <img src=x onerror="window.__hit = 1">`;
    vi.mocked(ipc.prepareLogin).mockResolvedValueOnce({ status: "ready", detail: hostile });
    const { getByTestId, findByTestId, container } = renderSection({ ...runsView(), credential: credentialOf("login", "unchecked") });
    await fireEvent.click(getByTestId("log-in"), { detail: 1 });
    expect((await findByTestId("login-command")).textContent).toBe(hostile);
    expect(container.querySelector("img")).toBeNull();
    await fireEvent.click(getByTestId("credential-copy"), { detail: 0 });
    expect(writeText).toHaveBeenCalledWith(hostile);
  });

  it("each credential state shows its action", () => {
    const states = Object.keys(CREDENTIAL_STATE_TEXT) as CredentialState[];
    for (const kind of ["oauth-token", "login"] as const) {
      for (const state of states) {
        const credential = credentialOf(kind, state)!;
        const { getByTestId, queryByTestId } = renderSection({ ...runsView(), credential });
        expect(getByTestId("credential-state").dataset.state, `${kind} ${state}`).toBe(state);
        const shown = queryByTestId("credential-action");
        expect(shown?.dataset.kind ?? null, `${kind} ${state}`).toBe(credential.action?.kind ?? null);
        if (credential.action?.kind === "setup_token") {
          expect(shown!.textContent).toContain(credential.action.cmdkey_line);
          expect(shown!.textContent).toContain(credential.action.step);
        }
        expect(queryByTestId("log-in") !== null, `${kind} ${state}`).toBe(kind === "login");
        cleanup();
      }
    }
  });

  it("stop all is keyboard reachable", async () => {
    const view = runsView();
    const { getByTestId, findByTestId } = renderSection(view);
    const stop = getByTestId("stop-all");
    expect(stop.tabIndex).toBe(0);
    await fireEvent.click(stop, { detail: 0 });
    expect(ipc.stopWorkshop).toHaveBeenCalledTimes(1);
    expect((await findByTestId("stop-answer")).textContent).toContain("1.2 s");
    cleanup();
    const board = new BoardStore();
    board.board = { ...fixtureOf<Board>("workshop", "workshop-runs-board"), workshop_live: 2 };
    const header = render(Header, { store: board, onView: vi.fn() });
    const inHeader = header.getByTestId("header-stop-all");
    expect(inHeader.tabIndex).toBe(0);
    await fireEvent.click(inHeader, { detail: 0 });
    expect(ipc.stopWorkshop).toHaveBeenCalledTimes(2);
  });

  it("a kept run and a labelled orphan can be removed by keyboard", async () => {
    const view = runsView();
    const orphanRun = "f".repeat(32);
    const { getAllByTestId, getByTestId } = renderSection({
      ...view,
      runs: {
        ...view.runs,
        orphans: [
          { name: `officina-worker-${orphanRun}`, run_id: orphanRun },
          { name: `officina-gateway-${orphanRun}`, run_id: orphanRun },
          { name: "officina-worker-unlabelled", run_id: null },
        ],
      },
    });
    const orphans = getAllByTestId("run-orphan");
    expect(orphans.map((o) => o.dataset.run)).toEqual([orphanRun, ""]);
    expect(orphans[1]!.querySelector("button")).toBeNull();
    await fireEvent.click(getByTestId("run-kept").querySelector("button")!, { detail: 0 });
    await fireEvent.click(orphans[0]!.querySelector("button")!, { detail: 0 });
    expect(vi.mocked(ipc.removeRun).mock.calls).toEqual([[view.runs.kept[0]!.id], [orphanRun]]);
  });
});
