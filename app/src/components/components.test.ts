import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/svelte";
import AlertBand from "./AlertBand.svelte";
import FocusList from "./FocusList.svelte";
import ApprovalCard from "./ApprovalCard.svelte";
import Gauge from "./Gauge.svelte";
import { tick } from "svelte";
import SessionCard from "./SessionCard.svelte";
import SessionFile from "./SessionFile.svelte";
import EndedCounts from "./EndedCounts.svelte";
import Briefing from "./Briefing.svelte";
import type { Briefing as BriefingPayload } from "../lib/generated/Briefing";
import StableLabel from "./StableLabel.svelte";
import { OUTCOME_TEXT, outcomeText, type OutcomeKind } from "../lib/outcome";
import { SIGN_WORDS, ZONE_WORDS } from "../lib/scene/text";
import { hours } from "../lib/time";
import type { FocusOutcome } from "../lib/generated/FocusOutcome";
import SubjectPicker, { type PickerChoice } from "./SubjectPicker.svelte";
import TriagePanel from "./TriagePanel.svelte";
import TriageRow from "./TriageRow.svelte";
import { BoardStore } from "../lib/board.svelte";
import { HALO_BREATHING_MS, HALO_HALF_MS, haloMemo, haloTiming } from "../lib/halo";
import * as ipc from "../lib/ipc";
import { RegistryStore, refusalText } from "../lib/registry.svelte";
import { AUTHOR_REFUSAL_TEXT, LIVE_STATE_TEXT, REASON_TEXT } from "../lib/registry-text";
import { RELAY_TOGGLE_TEXT } from "../lib/relay-text";
import { alertedCards, CAUSE_TEXT, causeText, LAST_KNOWN, waitText } from "../lib/alert";
import type { ApprovalView } from "../lib/generated/ApprovalView";
import type { Board } from "../lib/generated/Board";
import { fixtureOf, fixturesOf } from "../../e2e/fixture-index";
import type { AuthorRefusal } from "../lib/generated/AuthorRefusal";
import type { Card } from "../lib/generated/Card";
import type { Column } from "../lib/generated/Column";
import type { IpcError } from "../lib/generated/IpcError";
import type { ReasonCode } from "../lib/generated/ReasonCode";
import type { RegistryView } from "../lib/generated/RegistryView";
import type { SubjectView } from "../lib/generated/SubjectView";
import type { TriageRow as Row } from "../lib/generated/TriageRow";
// The generated union itself, read as text: its values come from Rust's
// `ReasonCode::ALL`, so a new code is iterated here without a list to edit.
import reasonCodeSource from "../lib/generated/ReasonCode.ts?raw";
import launchRefusalSource from "../lib/generated/LaunchRefusal.ts?raw";
import Composer from "./Composer.svelte";
import WorkOrderCard from "./WorkOrderCard.svelte";
import { OrchestratorStore } from "../lib/orchestrator.svelte";
import { LAUNCH_REFUSAL_TEXT } from "../lib/orchestrator-text";
import type { LaunchRefusal } from "../lib/generated/LaunchRefusal";
import type { OrchestratorView } from "../lib/generated/OrchestratorView";
import type { WorkOrderCard as WorkOrderCardPayload } from "../lib/generated/WorkOrderCard";

vi.mock("../lib/ipc", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../lib/ipc")>()),
  decideApproval: vi.fn(async () => undefined),
  confirmTriage: vi.fn(async () => []),
}));

const NOW = 1_800_000_000_000;

afterEach(cleanup);

function card(overrides: Partial<Card> = {}): Card {
  return {
    key: "pid:1:1",
    column: "needs_you",
    badge: null,
    kind: "interactive",
    name: "billing-api",
    project: { name: "billing-api", color: "blue", priority: "normal" },
    session_id: "s",
    background_id: null,
    pid: 1,
    cwd: "C:/work/billing-api",
    waiting_for: "permission prompt",
    last_prompt: null,
    last_message: null,
    last_message_tail: null,
    last_event_at_ms: NOW - 4 * 60_000,
    started_at_ms: NOW - 60 * 60_000,
    attach_available: false,
    subject: null,
    scene: null,
    settled: null,
    subagents: [],
    ...overrides,
  };
}

function store(): BoardStore {
  const s = new BoardStore();
  s.nowMs = NOW;
  return s;
}

describe("SessionCard", () => {
  it("shows name, waiting reason and times", () => {
    const { getByText, getByTestId } = render(SessionCard, { card: card(), store: store() });
    expect(getByText("billing-api", { selector: ".name" })).toBeTruthy();
    expect(getByTestId("waiting-for").textContent).toBe("permission prompt");
    expect(getByText("4 min ago")).toBeTruthy();
    expect(getByText("running 1 h")).toBeTruthy();
  });

  it("renders session text as text, never as markup", () => {
    const hostile = `<img src=x onerror="alert(1)"><b>x</b>`;
    const { container, getByTestId } = render(SessionCard, {
      card: card({ last_prompt: hostile, last_message: hostile, name: hostile }),
      store: store(),
    });
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("b")).toBeNull();
    expect(getByTestId("last-prompt").textContent).toBe(hostile);
  });

  it("numbers Needs you cards for the keyboard and offers attach on background cards", () => {
    const { getByText, queryByText } = render(SessionCard, {
      card: card({ kind: "background", background_id: "abc", attach_available: true }),
      store: store(),
      index: 0,
    });
    expect(getByText("1")).toBeTruthy();
    expect(getByText("Copy attach command")).toBeTruthy();
    expect(queryByText("Wrong window?")).toBeNull();
  });

  it("shows the badge of an unknown state", () => {
    const { getByTestId } = render(SessionCard, {
      card: card({ column: "working", waiting_for: null, badge: "unknown" }),
      store: store(),
    });
    expect(getByTestId("badge").textContent).toBe("unknown");
  });

  describe("halo", () => {
    afterEach(() => {
      haloMemo.retain([]);
      vi.restoreAllMocks();
    });

    function haloStyle(container: HTMLElement): { delay: string; count: string; half: string } {
      const article = container.querySelector<HTMLElement>("[data-testid=card]")!;
      return {
        delay: article.style.getPropertyValue("--halo-delay"),
        count: article.style.getPropertyValue("--halo-count"),
        half: article.style.getPropertyValue("--halo-half"),
      };
    }

    it("a Needs you card gets its halo timing once", async () => {
      const now = vi.spyOn(Date, "now").mockReturnValue(NOW);
      const waiting = card({ last_event_at_ms: NOW - 30_000 });
      const s = store();
      const { container, rerender } = render(SessionCard, { card: waiting, store: s });
      const timing = haloTiming(NOW - 30_000, NOW);
      expect(timing.iterations).toBeGreaterThan(0);
      expect(haloStyle(container)).toEqual({
        delay: `${timing.startMs - NOW}ms`,
        count: String(timing.iterations),
        half: `${HALO_HALF_MS}ms`,
      });
      const written = haloStyle(container);
      const reads = now.mock.calls.length;
      // A later board with the same wait: nothing rewritten, no clock read.
      now.mockReturnValue(NOW + 25_000);
      await rerender({ card: { ...waiting, waiting_for: "question" }, store: s });
      expect(haloStyle(container)).toEqual(written);
      expect(now).toHaveBeenCalledTimes(reads);
    });

    it("cards outside Needs you get no halo timing", () => {
      vi.spyOn(Date, "now").mockReturnValue(NOW);
      // Every column, from an exhaustive record over the generated union.
      const others = (Object.keys(LIVE_STATE_TEXT) as Column[]).filter((column) => column !== "needs_you");
      expect(others.length).toBeGreaterThan(0);
      for (const column of others) {
        const { container } = render(SessionCard, { card: card({ column, waiting_for: null }), store: store() });
        expect(haloStyle(container), column).toEqual({ delay: "", count: "", half: "" });
        cleanup();
      }
    });

    it("a waiting card without times breathes from first sight", () => {
      vi.spyOn(Date, "now").mockReturnValue(NOW);
      const { container } = render(SessionCard, {
        card: card({ last_event_at_ms: null, started_at_ms: null }),
        store: store(),
      });
      const timing = haloTiming(NOW, NOW);
      expect(haloStyle(container).count).toBe(String(timing.iterations));
      expect(haloStyle(container).delay).toBe(`${timing.startMs - NOW}ms`);
    });

    it("a waiting card older than the breathing time is steady", () => {
      vi.spyOn(Date, "now").mockReturnValue(NOW);
      const { container } = render(SessionCard, {
        card: card({ last_event_at_ms: null, started_at_ms: NOW - HALO_BREATHING_MS }),
        store: store(),
      });
      expect(haloStyle(container).count).toBe("0");
    });
  });
});

describe("StableLabel", () => {
  it("keeps every label in place and exposes only the current one", async () => {
    const { container, rerender } = render(StableLabel, { labels: RELAY_TOGGLE_TEXT, current: "running" });
    const all = () => Array.from(container.querySelectorAll("[data-stable-label] > *"));
    const exposed = () => all().filter((label) => label.getAttribute("aria-hidden") !== "true");
    for (const [state, text] of Object.entries(RELAY_TOGGLE_TEXT) as [keyof typeof RELAY_TOGGLE_TEXT, string][]) {
      await rerender({ labels: RELAY_TOGGLE_TEXT, current: state });
      expect(all().map((label) => label.textContent)).toEqual(Object.values(RELAY_TOGGLE_TEXT));
      expect(exposed().map((label) => label.textContent)).toEqual([text]);
    }
  });

  it("names its button after the current label only", async () => {
    const s = store();
    const background = card({ kind: "background", background_id: "abc", attach_available: true });
    const { getByRole, queryByRole } = render(SessionCard, { card: background, store: s });
    expect(getByRole("button", { name: "Copy attach command" })).toBeTruthy();
    s.copied = background.key;
    await tick();
    expect(getByRole("button", { name: "Copied" })).toBeTruthy();
    expect(queryByRole("button", { name: "Copy attach command" })).toBeNull();
  });
});

describe("Gauge", () => {
  const base = { label: "5 h", weekly: false, staleAfterMs: 30 * 60_000, nowMs: NOW };

  it("says there is no data instead of showing zero", () => {
    const { getByText } = render(Gauge, { ...base, window: null, writtenAtMs: null });
    expect(getByText("--")).toBeTruthy();
    expect(getByText("no data: see officina doctor")).toBeTruthy();
  });

  it("flags stale data with its age", () => {
    const { getByText } = render(Gauge, {
      ...base,
      window: { used_percentage: 62.4, resets_at: NOW / 1000 + 3600 },
      writtenAtMs: NOW - 45 * 60_000,
    });
    expect(getByText("62 %")).toBeTruthy();
    expect(getByText(/^stale, updated 45 min ago/)).toBeTruthy();
  });

  it("never shows a percentage once the window has reset", () => {
    const { getByText } = render(Gauge, {
      ...base,
      window: { used_percentage: 90, resets_at: NOW / 1000 - 60 },
      writtenAtMs: NOW - 5 * 60_000,
    });
    expect(getByText("--")).toBeTruthy();
    expect(getByText(/^reset, no new data/)).toBeTruthy();
  });

  it("says it on one line, stale first, the whole line in its tooltip", () => {
    const { getByText, getByRole } = render(Gauge, {
      ...base,
      window: { used_percentage: 62.4, resets_at: NOW / 1000 + 3600 },
      writtenAtMs: NOW - 45 * 60_000,
    });
    expect(getByText("62 %")).toBeTruthy();
    const line = getByText(/^stale, updated 45 min ago, resets at /);
    expect(line.getAttribute("title")).toBe(line.textContent);
    expect(getByRole("meter").getAttribute("aria-valuenow")).toBe("62");
  });
});

/** What a card gets besides its approval: its own state and decision. */
const onDecide = vi.fn();
function decision() {
  return { busy: false, error: undefined, nowMs: NOW, onDecide };
}

function approval(overrides: Partial<ApprovalView> = {}): ApprovalView {
  return {
    id: "0123456789abcdef-1",
    project: "infra",
    action: "restart",
    class: "human",
    state: "pending",
    escalation: null,
    description: "Restart one service",
    argv: ["C:/path/to/tool.exe", "restart", "web01"],
    http: null,
    valid: true,
    invalid_reason: null,
    session_id: "s",
    session_name: "deploy-lab",
    requested_at_ms: NOW - 3 * 60_000,
    expires_at_ms: NOW + 57 * 60_000,
    dry_run: null,
    binding: "not_git_tree",
    bound_files: null,
    files_unchanged: null,
    ...overrides,
  };
}

describe("ApprovalCard", () => {
  beforeEach(() => onDecide.mockClear());

  it("renders one chip per argument", () => {
    const argv = ["C:/path/to/tool.exe", "--limit", "lab web", "playbooks/site.yml"];
    const { getAllByTestId, getByTestId, getByText } = render(ApprovalCard, {
      approval: approval({ argv }),
      ...decision(),
    });
    expect(getAllByTestId("arg").map((chip) => chip.textContent)).toEqual(argv);
    expect(getByTestId("approval-session").textContent).toBe("deploy-lab");
    expect(getByText("asked 3 min ago")).toBeTruthy();
    expect(getByText("expires in 57 min")).toBeTruthy();
    cleanup();
    const unknown = render(ApprovalCard, { approval: approval({ session_name: null }), ...decision() });
    expect(unknown.getByTestId("approval-session").textContent).toBe("session unknown");
  });

  it("bidi and markup in arguments stay inert text", () => {
    const argv = [`<img src=x onerror="window.__hit = 1">`, "\u202Egnp.exe", "a b"];
    const { container, getAllByTestId } = render(ApprovalCard, {
      approval: approval({ argv, description: "<b>bold</b>" }),
      ...decision(),
    });
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("b")).toBeNull();
    const chips = getAllByTestId("arg");
    expect(chips.map((chip) => chip.textContent)).toEqual(argv);
    expect(chips.every((chip) => chip.children.length === 0)).toBe(true);
  });

  it("an HTTP request shows its method, URL and credential as inert text", () => {
    const url = `https://monitoring.example.com/api/<img src=x onerror="window.__hit = 1">`;
    const { container, getByTestId, queryByTestId } = render(ApprovalCard, {
      approval: approval({ argv: [], http: { method: "GET", url, credential: "monitoring" } }),
      ...decision(),
    });
    expect(getByTestId("http-method").textContent).toBe("GET");
    expect(getByTestId("http-url").textContent).toBe(url);
    expect(getByTestId("http-credential").textContent).toBe("credential monitoring");
    expect(queryByTestId("argv")).toBeNull();
    expect(container.querySelector("img")).toBeNull();
  });

  it("a pointer click calls decideApproval with the card id", async () => {
    const { getByTestId } = render(ApprovalCard, { approval: approval(), ...decision() });
    await fireEvent.click(getByTestId("approve"), { detail: 1 });
    expect(onDecide).toHaveBeenCalledWith(true);
    await fireEvent.click(getByTestId("reject"), { detail: 1 });
    expect(onDecide).toHaveBeenLastCalledWith(false);
  });

  it("Enter and Space on a focused Approve button do not decide", async () => {
    const { getByTestId } = render(ApprovalCard, { approval: approval(), ...decision() });
    for (const button of [getByTestId("approve"), getByTestId("reject")]) {
      button.focus();
      for (const key of ["Enter", " "]) {
        const keydown = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
        button.dispatchEvent(keydown);
        expect(keydown.defaultPrevented).toBe(true);
      }
      // What a browser does for a key activation: a click with detail 0.
      await fireEvent.click(button, { detail: 0 });
    }
    expect(onDecide).not.toHaveBeenCalled();
  });

  it("an invalid request shows its reason and no button", () => {
    const { queryByTestId, getByTestId } = render(ApprovalCard, {
      approval: approval({ valid: false, invalid_reason: "its key does not match", argv: [] }),
      ...decision(),
    });
    expect(getByTestId("invalid-reason").textContent).toContain("its key does not match");
    expect(queryByTestId("approve")).toBeNull();
    expect(queryByTestId("reject")).toBeNull();
    expect(queryByTestId("argv")).toBeNull();
  });

  it("ApprovalCard shows the dry run as inert text", () => {
    const output = `TASK [restart] <script>window.__hit = 1</script>\nchanged: [web01]\n`;
    const { container, getByTestId, rerender } = render(ApprovalCard, {
      approval: approval({ dry_run: { check: "check", run_id: "r1", output, missing: null } }),
      ...decision(),
    });
    expect(container.querySelector("script")).toBeNull();
    expect(getByTestId("dry-run-output").textContent).toBe(output);
    rerender({
      approval: approval({ dry_run: { check: "check", run_id: "r1", output: null, missing: "not kept" } }),
      ...decision(),
    });
    expect(getByTestId("dry-run-missing").textContent).toBe("not kept");
  });

  // D-170, D-173: the card says what the files are bound to, never "tree
  // unchanged": since the dry run, since the request, or not at all.
  it("ApprovalCard states its binding", () => {
    const line = (overrides: Partial<ApprovalView>): { text: string; changed: boolean } => {
      const { getByTestId } = render(ApprovalCard, { approval: approval(overrides), ...decision() });
      const element = getByTestId("binding");
      const result = { text: element.firstChild?.textContent?.trim() ?? "", changed: element.classList.contains("changed") };
      cleanup();
      return result;
    };
    const dryRun = { check: "check", run_id: "r1", output: "ok", missing: null };
    expect(line({ binding: "dry_run", dry_run: dryRun, bound_files: 12, files_unchanged: true })).toEqual({
      text: "tracked and unignored files unchanged since the dry run",
      changed: false,
    });
    expect(line({ binding: "request", bound_files: 12, files_unchanged: true })).toEqual({
      text: "tracked and unignored files unchanged since the request",
      changed: false,
    });
    expect(line({ binding: "request", bound_files: 12, files_unchanged: false })).toEqual({
      text: "tracked or unignored files changed since the request: the run will refuse this approval",
      changed: true,
    });
    expect(line({ binding: "request", bound_files: 12, files_unchanged: null }).text).toBe(
      "tracked and unignored files bound to the request",
    );
    expect(line({ binding: "not_git_tree" })).toEqual({ text: "no file binding: not a git tree", changed: false });
    const counted = render(ApprovalCard, {
      approval: approval({ binding: "request", bound_files: 12, files_unchanged: true }),
      ...decision(),
    });
    expect(counted.getByTestId("bound-files").textContent?.trim()).toBe("(12 files)");
    cleanup();
    const invalid = render(ApprovalCard, {
      approval: approval({ valid: false, invalid_reason: "forged", argv: [], binding: null }),
      ...decision(),
    });
    expect(invalid.queryByTestId("binding")).toBeNull();
  });

  // Story 5.12 AC 6 (D-28): Reject on an `auto` card asks why; neither it
  // nor a reason button acts on a key's synthesised click (detail 0).
  it("the reason buttons ignore a click with detail 0", async () => {
    const { getByTestId } = render(ApprovalCard, { approval: approval({ class: "auto" }), ...decision() });
    const reasons = getByTestId("reject-reasons");
    expect(reasons.classList.contains("shown")).toBe(false);
    await fireEvent.click(getByTestId("reject"), { detail: 0 });
    expect(reasons.classList.contains("shown")).toBe(false);
    await fireEvent.click(getByTestId("reject"), { detail: 1 });
    expect(reasons.classList.contains("shown")).toBe(true);
    expect(onDecide).not.toHaveBeenCalled();
    for (const reason of ["should_not_run", "not_now"]) {
      await fireEvent.click(getByTestId(`reason-${reason}`), { detail: 0 });
    }
    expect(onDecide).not.toHaveBeenCalled();
    await fireEvent.click(getByTestId("reason-not_now"), { detail: 1 });
    expect(onDecide).toHaveBeenCalledWith(false, "not_now");
  });

  it("a human card's Reject decides at once and has no reason slot", async () => {
    const { getByTestId, queryByTestId } = render(ApprovalCard, { approval: approval(), ...decision() });
    expect(queryByTestId("reject-reasons")).toBeNull();
    expect(getByTestId("approval-class").textContent?.trim()).toBe("class human");
    await fireEvent.click(getByTestId("reject"), { detail: 1 });
    expect(onDecide).toHaveBeenCalledWith(false);
  });
});

function subject(id: number, project: string, name: string): SubjectView {
  return {
    id,
    project,
    name,
    status: "active",
    status_src: "author",
    status_at_ms: NOW,
    routine: false,
    sessions: [],
    excerpts: null,
  };
}

describe("SubjectPicker", () => {
  it("lists the project's open subjects first and accepts a new name", async () => {
    const picks: PickerChoice[] = [];
    const subjects = [
      subject(1, "web-portal", "token refresh"),
      subject(2, "billing-api", "invoice export"),
      subject(3, "docs-site", "release notes"),
      subject(4, "billing-api", "flaky retry"),
    ];
    const { getAllByTestId, getByTestId } = render(SubjectPicker, {
      subjects,
      project: "billing-api",
      label: "File under",
      onPick: (choice: PickerChoice) => picks.push(choice),
    });
    const order = getAllByTestId("pick-subject").map((b) => Number(b.getAttribute("data-subject")));
    expect(order).toEqual([2, 4, 1, 3]);

    await fireEvent.click(getAllByTestId("pick-subject")[1]!);
    expect(picks).toEqual([{ subject: 4 }]);

    const create = getByTestId("picker-create") as HTMLButtonElement;
    expect(create.disabled).toBe(true);
    await fireEvent.input(getByTestId("picker-new-name"), { target: { value: "  csv export " } });
    expect(create.disabled).toBe(false);
    await fireEvent.click(create);
    expect(picks).toEqual([{ subject: 4 }, { name: "  csv export " }]);
  });

  it("renders subject names as text", () => {
    const hostile = `<img src=x onerror="alert(1)">`;
    const { container, getByText } = render(SubjectPicker, {
      subjects: [subject(1, "docs-site", hostile)],
      project: null,
      label: "File under",
      onPick: () => undefined,
    });
    expect(container.querySelector("img")).toBeNull();
    expect(getByText(hostile)).toBeTruthy();
  });
});

describe("SessionCard subject", () => {
  it("SessionCard renders its subject as text", () => {
    const hostile = `<img src=x onerror="alert(1)"><b>rollout</b>`;
    const { container, getByTestId } = render(SessionCard, {
      card: card({ subject: { id: 7, name: hostile } }),
      store: store(),
    });
    expect(getByTestId("card-subject").textContent).toBe(hostile);
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("b")).toBeNull();
  });

  it("shows no subject line on a session filed nowhere", () => {
    const { queryByTestId } = render(SessionCard, { card: card(), store: store() });
    expect(queryByTestId("card-subject")).toBeNull();
  });
});

function triageRow(overrides: Partial<Row> = {}): Row {
  return {
    key: "subject:1",
    kind: "subject",
    project: "billing-api",
    subject: { id: 1, name: "invoice export" },
    session_id: "s1",
    session_name: "invoice-export",
    proposal: { verdict: "to_resume", source: "rules", reason: "default_to_resume", call_id: null },
    verdict: "to_resume",
    excerpts: null,
    end: null,
    proposed_subject: null,
    choices: [],
    model_pending: false,
    model_reason: null,
    model_call: null,
    ...overrides,
  };
}

function registryWith(rows: Row[]): RegistryStore {
  const view: RegistryView = {
    schema: 1,
    generated_at_ms: NOW,
    registry: { state: "ready", detail: null, exclude_from_sync: null },
    ingestion: { pending_bytes: 0, incomplete_files: 0, rewritten_files: 0, corrupt_lines: 0 },
    subjects: [],
    unfiled: [],
    projects: [],
    triage: rows,
    to_sort: rows.length,
    restore_notice: null,
    model: {
      enabled: false,
      src: null,
      at_ms: null,
      reason: null,
      unavailable: null,
      paused_at_ms: null,
      pending: 0,
      latest_dry_run: null,
      dry_run: null,
      last_stop: null,
      calls: {
        today: { calls: 0, unanswered: 0, refused: 0, input_tokens: 0, output_tokens: 0, cost_microusd: 0 },
        last_30_days: { calls: 0, unanswered: 0, refused: 0, input_tokens: 0, output_tokens: 0, cost_microusd: 0 },
        refusals: [],
      },
    },
    rates: [],
    gates: {
      auto_archive: { state: "not_enough_cases", cases: 0, needed: 50 },
      model_enable: { state: "no_dry_run", cases: 0, needed: 30 },
      model_self_disable: { state: "holds", cases: 0, needed: 20 },
    },
    auto_archive: { on: false, src: null, at_ms: null, reason: null },
    recent_auto_archives: [],
    routine_candidates: [],
    thresholds: {
      count_window_ms: 30 * 86_400_000,
      auto_count_after_ms: 7 * 86_400_000,
      pause_after_failed_calls: 3,
    },
  };
  const store = new RegistryStore();
  store.view = view;
  return store;
}

describe("TriageRow", () => {
  it("TriageRow renders the reason wording of every reason code", () => {
    const codes = [...reasonCodeSource.matchAll(/"([a-z_]+)"/g)].map((m) => m[1] as ReasonCode);
    expect(codes.length).toBeGreaterThan(0);
    expect(new Set(codes)).toEqual(new Set(Object.keys(REASON_TEXT)));
    for (const reason of codes) {
      const row = triageRow({ proposal: { verdict: "to_resume", source: "rules", reason, call_id: null } });
      const { getByTestId } = render(TriageRow, { row, store: registryWith([row]) });
      expect(getByTestId("triage-reason").textContent).toBe(REASON_TEXT[reason]);
      expect(REASON_TEXT[reason].length).toBeGreaterThan(0);
      cleanup();
    }
  });
});

describe("TriagePanel", () => {
  it("Enter and Space on Confirm all do nothing", async () => {
    const confirm = vi.mocked(ipc.confirmTriage);
    confirm.mockClear();
    const { getByTestId } = render(TriagePanel, { store: registryWith([triageRow()]) });
    const button = getByTestId("confirm-all");
    button.focus();
    for (const key of ["Enter", " "]) {
      const keydown = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
      button.dispatchEvent(keydown);
      expect(keydown.defaultPrevented).toBe(true);
    }
    // What a browser does for a key activation: a click with detail 0.
    await fireEvent.click(button, { detail: 0 });
    expect(confirm).not.toHaveBeenCalled();
    // Positive control: a pointer click confirms.
    await fireEvent.click(button, { detail: 1 });
    expect(confirm).toHaveBeenCalledTimes(1);
  });
});

describe("refusalText", () => {
  it("words the rule that refused an author action, and passes other errors through", () => {
    for (const refusal of Object.keys(AUTHOR_REFUSAL_TEXT) as AuthorRefusal[]) {
      const error: IpcError = { code: "author_refused", message: `refused: ${refusal}`, refusal };
      expect(refusalText(error)).toBe(AUTHOR_REFUSAL_TEXT[refusal]);
    }
    expect(refusalText({ code: "timeout", message: "too slow", refusal: null })).toBe("too slow");
    expect(refusalText(new Error("boom"))).toBe("Error: boom");
  });
});

describe("AlertBand", () => {
  /** A scene fixture board, read through the fixture index (D-181). */
  function sceneBoard(name: string): Board {
    const found = fixturesOf<Board>("board").find((f) => f.name === name);
    if (!found) throw new Error(`no fixture ${name}: run pnpm e2e:fixtures`);
    return found.data;
  }

  it("AlertBand renders session text as text", () => {
    const board = sceneBoard("scene-alert");
    const hostile = `<img src=x onerror="window.__officinaXss = 1">`;
    const first = alertedCards(board)[0]!;
    const named = {
      ...board,
      columns: {
        ...board.columns,
        needs_you: board.columns.needs_you.map((c) => (c.key === first.key ? { ...c, name: hostile } : c)),
      },
    };
    const { getByTestId, container } = render(AlertBand, { board: named, nowMs: board.generated_at_ms });
    expect(getByTestId("alert-band-text").textContent).toContain(hostile);
    expect(container.querySelector("img")).toBeNull();
  });

  it("AlertBand words both causes", () => {
    const board = sceneBoard("scene-alert");
    const causes = new Set(alertedCards(board).map((c) => c.scene!.alert));
    expect([...causes].sort()).toEqual(Object.keys(CAUSE_TEXT).sort());
    for (const card of alertedCards(board)) {
      const words = causeText(card, board.generated_at_ms);
      expect(words.startsWith(CAUSE_TEXT[card.scene!.alert!])).toBe(true);
      if (card.scene!.alert === "waited") {
        const minutes = Math.floor((board.generated_at_ms - card.scene!.waiting_since_ms!) / 60_000);
        expect(words).toBe(`${CAUSE_TEXT.waited} ${minutes} min`);
      }
    }
    const { getByTestId } = render(AlertBand, { board, nowMs: board.generated_at_ms });
    expect(getByTestId("alert-band-text").textContent).toContain(", and 1 more.");
  });

  it("AlertBand marks a last known alert on unknown data", () => {
    const board = sceneBoard("scene-alert");
    const first = alertedCards(board)[0]!;
    const unknown = {
      ...board,
      columns: {
        ...board.columns,
        needs_you: board.columns.needs_you.map((c) =>
          c.key === first.key ? { ...c, scene: { ...c.scene!, zone: "unknown" as const } } : c,
        ),
      },
    };
    const { getByTestId } = render(AlertBand, { board: unknown, nowMs: board.generated_at_ms });
    expect(getByTestId("alert-band-text").textContent!.endsWith(LAST_KNOWN)).toBe(true);
    cleanup();
    const fresh = render(AlertBand, { board, nowMs: board.generated_at_ms });
    expect(fresh.getByTestId("alert-band-text").textContent).not.toContain(LAST_KNOWN);
  });

  it("AlertBand is absent without an alert", () => {
    const board = sceneBoard("scene-c");
    expect(board.alert_count).toBe(0);
    const { queryByTestId } = render(AlertBand, { board, nowMs: board.generated_at_ms });
    expect(queryByTestId("alert-band")).toBeNull();
  });
});

describe("SessionFile", () => {
  /** A board holding `cards`, from a fixture board's shape. */
  function boardOf(cards: Card[], approvals: ApprovalView[] = []): Board {
    const base = fixturesOf<Board>("board")[0]!.data;
    const columns: Board["columns"] = { needs_you: [], your_turn: [], working: [], to_resume: [], done: [] };
    for (const c of cards) columns[c.column].push(c);
    return { ...base, columns, approvals, generated_at_ms: NOW };
  }

  function file(c: Card, approvals: ApprovalView[] = [], s: BoardStore = store()) {
    return render(SessionFile, {
      form: { kind: "file", card: c, back: false },
      board: boardOf([c], approvals),
      store: s,
      onClose: () => {},
      onOpen: () => {},
      onBoard: () => {},
    });
  }

  const waiting = (overrides: Partial<Card> = {}) =>
    card({ scene: { zone: "decision", waiting_since_ms: NOW - 12 * 60_000, alert: null, approval: false }, ...overrides });

  it("SessionFile renders session text as text", () => {
    const hostile = `<img src=x onerror="window.__officinaXss = 1"><b>x</b>`;
    const c = waiting({
      name: hostile,
      project: { name: hostile, color: null, priority: "high" },
      last_prompt: hostile,
      last_message: hostile,
      last_message_tail: hostile,
      waiting_for: hostile,
      subject: { id: 1, name: hostile },
    });
    const { container, getByRole, getByTestId } = file(c);
    expect(container.querySelector("img, b")).toBeNull();
    expect(getByRole("heading").textContent).toBe(hostile);
    for (const id of ["file-project", "file-waiting-for", "file-subject", "file-prompt", "file-answer", "file-answer-tail"]) {
      expect(getByTestId(id).textContent, id).toContain(hostile);
    }
    expect((window as { __officinaXss?: number }).__officinaXss).toBeUndefined();
  });

  it("SessionFile lists sub-agents as text", () => {
    expect(file(waiting()).queryByTestId("file-subagents")).toBeNull();
    cleanup();
    const hostile = `<img src=x onerror="window.__officinaXss = 1">`;
    const c = waiting({
      subagents: [
        { agent_id: "a1", agent_type: "Explore" },
        { agent_id: "a2", agent_type: hostile },
        { agent_id: "a3", agent_type: null },
      ],
    });
    const { container, getByTestId } = file(c);
    const items = [...getByTestId("file-subagents").querySelectorAll("li")].map((li) => li.textContent);
    expect(items).toEqual(["Explore", hostile, "sub-agent"]);
    expect(container.querySelector("img")).toBeNull();
  });

  it("SessionFile is a dialog labelled by the session name", () => {
    const { getByRole, getByTestId } = file(waiting());
    const dialog = getByRole("dialog");
    expect(dialog.hasAttribute("aria-modal")).toBe(false);
    const title = document.getElementById(dialog.getAttribute("aria-labelledby")!);
    expect(title?.textContent).toBe("billing-api");
    expect(document.activeElement).toBe(getByTestId("bring-to-front"));
  });

  it("SessionFile omits the subject line without a subject", () => {
    expect(file(waiting()).queryByTestId("file-subject")).toBeNull();
    cleanup();
    expect(file(waiting({ subject: { id: 3, name: "release 1.2" } })).getByTestId("file-subject").textContent).toBe("release 1.2");
  });

  it('SessionFile shows "since unknown" without a wait start', () => {
    const unknownStart = waiting({ scene: { zone: "decision", waiting_since_ms: null, alert: null, approval: false } });
    expect(file(unknownStart).getByTestId("file-since").textContent!.trim()).toBe("since unknown");
    cleanup();
    expect(file(waiting()).getByTestId("file-since").textContent).toContain("(12 min)");
    cleanup();
    // Not waiting: no wait line at all.
    const working = card({ column: "working", waiting_for: null, scene: { zone: "office", waiting_since_ms: null, alert: null, approval: false } });
    expect(file(working).queryByTestId("file-since")).toBeNull();
  });

  it("SessionFile shows its project's priority, its number and the zone", () => {
    const c = waiting({ project: { name: "billing-api", color: "green", priority: "high" } });
    const { getByTestId } = file(c);
    expect(getByTestId("file-project").textContent).toContain(SIGN_WORDS.highPriority);
    expect(getByTestId("file-number").textContent).toBe("1");
    expect(getByTestId("file-zone").textContent).toBe(ZONE_WORDS.decision);
  });

  it("SessionFile states an alert's cause, last known on unknown data", () => {
    const alerted = waiting({ scene: { zone: "alert", waiting_since_ms: NOW - 12 * 60_000, alert: "waited", approval: false } });
    expect(file(alerted).getByTestId("file-alert").textContent!.trim()).toBe(causeText(alerted, NOW));
    cleanup();
    const unknown = { ...alerted, scene: { ...alerted.scene!, zone: "unknown" as const } };
    expect(file(unknown).getByTestId("file-alert").textContent!.trim()).toBe(`${causeText(unknown, NOW)}, last known`);
  });

  it("SessionFile shows a pending approval and opens Board on it", async () => {
    const c = card({ column: "working", waiting_for: null, scene: { zone: "decision", waiting_since_ms: NOW - 5 * 60_000, alert: null, approval: true } });
    const request: ApprovalView = {
      id: "r1",
      project: c.project.name,
      action: "restart",
      class: "human",
      state: "pending",
      escalation: null,
      description: "Restart the service",
      argv: [],
      http: null,
      valid: true,
      invalid_reason: null,
      session_id: c.session_id,
      session_name: c.name,
      requested_at_ms: NOW - 5 * 60_000,
      expires_at_ms: NOW + 60_000,
      dry_run: null,
      binding: null,
      bound_files: null,
      files_unchanged: null,
    };
    const onBoard = vi.fn();
    const { getByTestId } = render(SessionFile, {
      form: { kind: "file", card: c, back: false },
      board: boardOf([c], [request]),
      store: store(),
      onClose: () => {},
      onOpen: () => {},
      onBoard,
    });
    expect(getByTestId("file-approval").textContent).toContain("restart");
    expect(getByTestId("file-waiting-for").textContent).toBe(waitText(c));
    expect(getByTestId("file-since").textContent).toContain("(5 min)");
    await fireEvent.click(getByTestId("open-in-board"));
    expect(onBoard).toHaveBeenCalledTimes(1);
  });

  it("SessionCard and SessionFile word outcomes alike", () => {
    for (const kind of Object.keys(OUTCOME_TEXT) as OutcomeKind[]) {
      const outcome = (
        kind === "candidates" ? { outcome: kind, detail: ["one", "two"] } : kind === "failed" ? { outcome: kind, detail: "boom" } : { outcome: kind }
      ) as FocusOutcome;
      const s = store();
      s.outcomes = { [card().key]: { outcome, atMs: NOW } };
      const cardText = render(SessionCard, { card: card(), store: s }).getByTestId("focus-outcome").textContent;
      cleanup();
      const fileText = file(waiting(), [], s).getByTestId("focus-outcome").textContent;
      cleanup();
      expect(cardText, kind).toBe(fileText);
      expect(cardText, kind).toContain(outcomeText(outcome));
    }
  });

  it("the list form lists its sessions, each a button that opens its file", async () => {
    const first = waiting({ key: "a", name: "first" });
    const second = waiting({ key: "b", name: "second" });
    const onOpen = vi.fn();
    const { getAllByRole, getByRole } = render(SessionFile, {
      form: { kind: "list", title: "Lounge", cards: [first, second], focusKey: "b" },
      board: boardOf([first, second]),
      store: store(),
      onClose: () => {},
      onOpen,
      onBoard: () => {},
    });
    const rows = getAllByRole("button").filter((b) => b.dataset.row !== undefined);
    expect(rows.map((r) => r.dataset.row)).toEqual(["a", "b"]);
    expect(document.activeElement).toBe(rows[1]);
    expect(getByRole("heading").textContent).toBe("Lounge");
    await fireEvent.click(rows[0]!);
    expect(onOpen).toHaveBeenCalledWith("a");
  });
});

describe("EndedCounts", () => {
  /** The words a count shows: its visible label, the room it keeps hidden. */
  const shown = (button: HTMLElement) => button.querySelector(".label:not(.inactive)")?.textContent;

  it("EndedCounts counts each column, words the Done window in its tooltip, and opens each column", async () => {
    for (const board of fixturesOf<Board>("board").map((f) => f.data)) {
      const onOpen = vi.fn();
      const { getByTestId } = render(EndedCounts, {
        toResume: board.columns.to_resume.length,
        done: board.columns.done.length,
        doneWindowMs: board.done_window_ms,
        onOpen,
      });
      expect(shown(getByTestId("ended-to-resume"))).toBe(`${board.columns.to_resume.length} to resume`);
      expect(shown(getByTestId("ended-done"))).toBe(`${board.columns.done.length} done`);
      expect(getByTestId("ended-done").getAttribute("title")).toContain(`in the last ${hours(board.done_window_ms)}:`);
      await fireEvent.click(getByTestId("ended-to-resume"));
      await fireEvent.click(getByTestId("ended-done"));
      expect(onOpen.mock.calls).toEqual([["to_resume"], ["done"]]);
      cleanup();
    }
    // Another window, another word: nothing is fixed at 24 h.
    const { getByTestId } = render(EndedCounts, { toResume: 0, done: 1, doneWindowMs: 12 * 3_600_000, onOpen: () => {} });
    expect(getByTestId("ended-done").getAttribute("title")).toContain("in the last 12 h:");
  });
});

describe("FocusList", () => {
  it("FocusList renders session text as text", () => {
    const hostile = `<img src=x onerror="alert(1)"><b>x</b>`;
    const base = fixturesOf<Board>("board")[0]!.data;
    const waiting = card({
      name: hostile,
      waiting_for: hostile,
      project: { name: hostile, color: "blue", priority: "high" },
      scene: { zone: "unknown", waiting_since_ms: NOW - 12 * 60_000, alert: "priority", approval: false },
    });
    const turn = card({ key: "pid:2:2", column: "your_turn", name: hostile, waiting_for: null });
    const board: Board = {
      ...base,
      columns: { needs_you: [waiting], your_turn: [turn], working: [], to_resume: [], done: [] },
      approvals: [],
    };
    const { container, getAllByTestId, getByTestId } = render(FocusList, { board, store: store(), onApprovals: () => {} });
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("b")).toBeNull();
    expect(getAllByTestId("focus-row").map((row) => row.querySelector(".name")?.textContent)).toEqual([hostile, hostile]);
    expect(getByTestId("focus-waiting-for").textContent).toBe(hostile);
    // The alert's cause, last known on unknown data (D-494).
    expect(getByTestId("focus-alert").textContent).toContain(`${CAUSE_TEXT.priority}, last known`);
  });
});

describe("Briefing", () => {
  const hostile = `<img src=x onerror="window.__officinaXss = 1">`;
  const section = <T,>(items: T[], dropped = 0) => ({ count: items.length + dropped, dropped, items });

  function briefing(): BriefingPayload {
    return {
      generated_at_ms: NOW,
      cursor: { kind: "none" },
      subjects: section([
        {
          id: 1,
          project: "billing-api",
          name: "invoice export",
          status: "active",
          sessions: [{ session_id: "s1", name: hostile, phase: "working" }],
          at_ms: NOW,
        },
      ]),
      waiting: section([
        {
          key: "k1",
          session_id: "s1",
          name: hostile,
          project: "billing-api",
          column: "needs_you",
          waiting_for: "permission prompt",
          subject: null,
          at_ms: NOW,
        },
      ]),
      approvals: section([
        {
          id: "r1",
          project: "billing-api",
          action: "deploy",
          session_name: hostile,
          valid: true,
          expires_at_ms: NOW + 60_000,
          at_ms: NOW,
        },
      ]),
      triage: section([
        { key: "session:s2", kind: "unfiled_session", project: "docs-site", subject: null, session_name: hostile, at_ms: null },
      ]),
      runs: section([
        {
          kind: "refused",
          project: "billing-api",
          session_id: "s1",
          action: hostile,
          exit_code: null,
          reason: "action_not_allowed",
          at_ms: NOW,
        },
      ]),
      work_orders: section([]),
      changed: section([{ kind: "started_waiting", key: "k1", name: hostile, column: "needs_you", at_ms: NOW }], 3),
    };
  }

  it("Briefing renders session names with markup as text", () => {
    const { container, getByTestId } = render(Briefing, { briefing: briefing(), nowMs: NOW });
    expect(container.querySelector("img")).toBeNull();
    for (const id of ["briefing-waiting", "briefing-approval", "briefing-triage", "briefing-run", "briefing-change", "briefing-subject"]) {
      expect(getByTestId(id).textContent, id).toContain(hostile);
    }
    expect(getByTestId("briefing-dropped-changed").textContent).toContain("3 older items");
    expect(getByTestId("briefing-since").textContent).toContain("first reading");
  });
});

describe("Composer", () => {
  function composer(): { store: OrchestratorStore; view: OrchestratorView } {
    const store = new OrchestratorStore();
    const view = fixtureOf<OrchestratorView>("orchestrator", "orchestrator-day");
    store.view = view;
    return { store, view };
  }

  /** The values a select offers: its options but the disabled placeholder. */
  const offered = (select: HTMLElement): string[] =>
    Array.from((select as HTMLSelectElement).options)
      .filter((o) => !o.disabled)
      .map((o) => o.value);

  it("Composer offers one select per parameter with its list", async () => {
    const { store, view } = composer();
    const { getByTestId, getAllByTestId, queryAllByTestId } = render(Composer, { store });
    const template = view.templates.find((t) => Object.keys(t.params).length > 1)!;
    expect(queryAllByTestId("composer-param")).toHaveLength(0);
    expect(offered(getByTestId("composer-template"))).toEqual(view.templates.map((t) => t.name));
    await fireEvent.change(getByTestId("composer-template"), { target: { value: template.name } });
    expect(offered(getByTestId("composer-project"))).toEqual(template.projects);
    const selects = getAllByTestId("composer-param");
    expect(selects.map((s) => s.dataset.param)).toEqual(Object.keys(template.params));
    for (const select of selects) {
      expect(select.tagName).toBe("SELECT");
      expect(offered(select)).toEqual(template.params[select.dataset.param!]);
      // The placeholder is no value: it cannot be chosen.
      for (const placeholder of Array.from((select as HTMLSelectElement).options).filter((o) => o.disabled)) {
        expect(placeholder.value).toBe("");
      }
    }
  });

  it("Composer has no text input for a value", () => {
    const { store } = composer();
    const card = fixtureOf<WorkOrderCardPayload>("card", "card-bump");
    store.choices = {
      template: card.template,
      project: card.project,
      values: Object.fromEntries(card.values.map((p) => [p.name, p.value])),
    };
    store.card = card;
    const { container, getByTestId } = render(Composer, { store });
    expect(getByTestId("subject-picker")).toBeTruthy();
    const typed = Array.from(container.querySelectorAll("input, textarea, [contenteditable]")).filter(
      (e) => !(e instanceof HTMLInputElement) || !["checkbox", "radio", "button", "submit"].includes(e.type),
    );
    // The only text field names a new subject, never a value.
    expect(typed.map((e) => e.getAttribute("data-testid"))).toEqual(["picker-new-name"]);
    expect(container.querySelectorAll("select")).toHaveLength(2 + card.values.length);
  });
});

describe("WorkOrderCard", () => {
  const onLaunch = vi.fn();
  beforeEach(() => onLaunch.mockReset());

  const bump = (): WorkOrderCardPayload => fixtureOf<WorkOrderCardPayload>("card", "card-bump");

  function props(card: WorkOrderCardPayload = bump()) {
    return { card, order: null, launching: false, launch: null, nowMs: NOW, onLaunch };
  }

  it("WorkOrderCard shows one chip per argument", () => {
    const card = bump();
    const { getAllByTestId, getByTestId } = render(WorkOrderCard, props(card));
    const chips = getAllByTestId("card-arg");
    expect(chips.map((chip) => chip.textContent)).toEqual(card.argv);
    expect(chips.every((chip) => chip.children.length === 0)).toBe(true);
    expect(getAllByTestId("card-value").map((chip) => chip.dataset.param)).toEqual(card.values.map((p) => p.name));
    expect(getByTestId("card-cwd").textContent).toContain(card.cwd);
  });

  it("WorkOrderCard renders the prompt as text", () => {
    const hostile = `<img src=x onerror="window.__hit = 1"> ‮gnp.exe`;
    const card = { ...bump(), prompt: hostile, description: "<b>bold</b>" };
    const { container, getByTestId } = render(WorkOrderCard, props(card));
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("b")).toBeNull();
    const prompt = getByTestId("card-prompt");
    expect(prompt.tagName).toBe("PRE");
    expect(prompt.textContent).toBe(hostile);
    expect(prompt.children).toHaveLength(0);
  });

  it("Launch ignores a click with detail 0", async () => {
    const { getByTestId } = render(WorkOrderCard, props());
    await fireEvent.click(getByTestId("launch"), { detail: 0 });
    expect(onLaunch).not.toHaveBeenCalled();
    await fireEvent.click(getByTestId("launch"), { detail: 1 });
    expect(onLaunch).toHaveBeenCalledTimes(1);
  });

  it("Enter and Space on Launch do nothing", async () => {
    const { getByTestId } = render(WorkOrderCard, props());
    const button = getByTestId("launch");
    expect(button.hasAttribute("tabindex")).toBe(false);
    button.focus();
    for (const key of ["Enter", " "]) {
      const keydown = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
      button.dispatchEvent(keydown);
      expect(keydown.defaultPrevented).toBe(true);
    }
    // What a browser does for a key activation: a click with detail 0.
    await fireEvent.click(button, { detail: 0 });
    expect(onLaunch).not.toHaveBeenCalled();
  });

  it("a launch in flight disables Launch and says so", () => {
    const { getByTestId } = render(WorkOrderCard, { ...props(), launching: true });
    const button = getByTestId("launch") as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    expect(button.querySelector("[data-stable-label]")).not.toBeNull();
    expect(getByTestId("card-launching")).toBeTruthy();
  });

  it("every launch refusal has a wording", () => {
    // The generated union, read as text: its values come from Rust's
    // `LaunchRefusal::ALL`, so a new refusal is iterated here.
    const refusals = Array.from(launchRefusalSource.matchAll(/"([a-z_]+)"/g), (m) => m[1]! as LaunchRefusal);
    expect(refusals.length).toBeGreaterThan(0);
    expect(Object.keys(LAUNCH_REFUSAL_TEXT).sort()).toEqual([...refusals].sort());
    const texts = refusals.map((r) => LAUNCH_REFUSAL_TEXT[r]);
    for (const text of texts) expect(text.trim()).not.toBe("");
    expect(new Set(texts).size).toBe(texts.length);
    for (const refusal of refusals) {
      cleanup();
      const { getByTestId } = render(WorkOrderCard, { ...props(), launch: { outcome: "refused", value: refusal } });
      expect(getByTestId("card-launch-refused").textContent).toBe(LAUNCH_REFUSAL_TEXT[refusal]);
      expect((getByTestId("launch") as HTMLButtonElement).disabled).toBe(true);
    }
  });
});
