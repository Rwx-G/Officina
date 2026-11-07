// Installed only in `--mode e2e` builds (AD-17): replaces the Rust backend
// with boards and relay views injected by Playwright, so the real UI runs in
// a browser.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import type { AutoMode } from "./lib/generated/AutoMode";
import type { BatteryAnswer } from "./lib/generated/BatteryAnswer";
import type { Board } from "./lib/generated/Board";
import type { ApprovalAnswer } from "./lib/generated/ApprovalAnswer";
import type { DiscardAnswer } from "./lib/generated/DiscardAnswer";
import type { ImportAnswer } from "./lib/generated/ImportAnswer";
import type { RemoveRunAnswer } from "./lib/generated/RemoveRunAnswer";
import type { PrepareLoginAnswer } from "./lib/generated/PrepareLoginAnswer";
import type { CheckLoginAnswer } from "./lib/generated/CheckLoginAnswer";
import type { ResumeAnswer } from "./lib/generated/ResumeAnswer";
import type { ResumeChoice } from "./lib/generated/ResumeChoice";
import type { StopAnswer } from "./lib/generated/StopAnswer";
import type { BuildAnswer } from "./lib/generated/BuildAnswer";
import type { CardAnswer } from "./lib/generated/CardAnswer";
import type { ConfirmItem } from "./lib/generated/ConfirmItem";
import type { IpcError } from "./lib/generated/IpcError";
import type { ItemAnswer } from "./lib/generated/ItemAnswer";
import type { LaunchAnswer } from "./lib/generated/LaunchAnswer";
import type { MeetingView } from "./lib/generated/MeetingView";
import type { AnswerResult } from "./lib/generated/AnswerResult";
import type { OrchestratorView } from "./lib/generated/OrchestratorView";
import type { WorkshopView } from "./lib/generated/WorkshopView";
import type { RegistryView } from "./lib/generated/RegistryView";
import type { RelayView } from "./lib/generated/RelayView";
import type { ReviewView } from "./lib/generated/ReviewView";
import type { StartAnswer } from "./lib/generated/StartAnswer";
import type { WorkOrderPermission } from "./lib/generated/WorkOrderPermission";

/** The author's commands on subjects (story 3.5): each is recorded, then
 *  refused when the test says so, else accepted with nothing changed; a test
 *  shows the outcome by pushing the next registry view. */
const AUTHOR_COMMANDS = [
  "create_subject",
  "rename_subject",
  "merge_subjects",
  "file_sessions",
  "archive_subject",
  "reopen_subject",
  "settle_session",
];

export function installE2eMock(): void {
  window.__officinaCalls = [];
  mockWindows("main");
  mockIPC(
    (cmd, args) => {
      window.__officinaCalls?.push({ cmd, args });
      switch (cmd) {
        case "get_board":
          return window.__officinaBoard ?? null;
        case "get_relay":
          return window.__officinaRelay ?? null;
        case "focus": {
          const key = (args as { key: string }).key;
          return window.__officinaFocus?.[key] ?? { outcome: "focused" };
        }
        case "copy_attach_command": {
          // Mirrors the backend: the command is built from the card's
          // background id, never from the key.
          const key = (args as { key: string }).key;
          const columns = window.__officinaBoard?.columns;
          const card = columns && Object.values(columns).flat().find((c) => c.key === key);
          return card?.background_id ? `claude attach ${card.background_id}` : null;
        }
        case "approve_catalogue":
          return approve((args as { hash: string }).hash);
        case "decide_approval": {
          const { id, approve, reason } = args as { id: string; approve: boolean; reason?: string };
          return decide(id, approve, reason);
        }
        case "set_relay_stopped":
          return setStopped((args as { stopped: boolean }).stopped);
        case "set_auto_approval":
          return setAutoApproval((args as { on: boolean }).on);
        case "set_auto_mode":
          return setAutoMode((args as { mode: AutoMode }).mode);
        case "mark_should_not_have_run":
          return markRun((args as { runId: string }).runId);
        case "acknowledge_anomaly":
          return acknowledge((args as { key: string }).key);
        case "get_registry":
          return window.__officinaRegistry ?? null;
        case "list_archived":
          return window.__officinaArchived ?? [];
        case "list_discarded":
          return window.__officinaDiscarded ?? [];
        case "confirm_triage":
          return confirmTriage((args as { items: ConfirmItem[] }).items);
        case "evaluate_triage_model":
          return evaluate();
        case "set_auto_archive":
          return setAutoArchive((args as { on: boolean }).on);
        case "set_triage_model":
          return setTriageModel((args as { on: boolean }).on);
        case "issue_card":
        case "edit_card":
        case "issue_card_from_proposal":
          return card();
        case "set_work_order_permission":
          return switched((args as { permission: WorkOrderPermission }).permission);
        case "dismiss_proposal":
          return dismissProposal((args as { proposalId: string }).proposalId);
        case "launch_work_order":
          return launch();
        case "get_orchestrator":
          return window.__officinaOrchestrator ?? null;
        case "get_workshop":
          return window.__officinaWorkshop ?? null;
        case "mark_briefing_read":
          return null;
        case "start_orchestrator":
          return startOrchestrator();
        case "open_meeting":
          return meetingOf((args as { key: string }).key);
        case "build_images":
          return buildImages();
        case "run_battery":
          return runBattery();
        case "open_review":
          return openReview((args as { runId: string }).runId);
        case "approve_sensitive":
          return approveSensitive();
        case "import_package":
          return importPackage((args as { runId: string }).runId);
        case "discard_package":
          return discardPackage();
        case "stop_workshop":
          return stopWorkshop();
        case "resume_workshop":
          return resumeWorkshop((args as { choices: ResumeChoice[] }).choices, (args as { resumeRelay: boolean }).resumeRelay);
        case "remove_run":
          return removeRun();
        case "prepare_login":
          return prepareLogin();
        case "check_login":
          return checkLogin();
        case "send_answer":
          return answer((args as { proposalId?: string }).proposalId ?? null);
        default:
          if (AUTHOR_COMMANDS.includes(cmd)) return author(cmd);
          return null;
      }
    },
    { shouldMockEvents: true },
  );
  window.__officinaPushBoard = async (board: Board) => {
    window.__officinaBoard = board;
    await emit("board://updated", board);
  };
  window.__officinaPushRelay = async (view: RelayView) => {
    window.__officinaRelay = view;
    await emit("relay://updated", view);
  };
  window.__officinaPushRegistry = async (view: RegistryView) => {
    window.__officinaRegistry = view;
    await emit("registry://updated", view);
  };
  window.__officinaPushWorkshop = async (view: WorkshopView) => {
    window.__officinaWorkshop = view;
    await emit("workshop://updated", view);
  };
  window.__officinaPushOrchestrator = async (view: OrchestratorView) => {
    window.__officinaOrchestrator = view;
    await emit("orchestrator://updated", view);
  };
}

/** Mirrors `send_answer` (AD-85): what the test set in
 *  `__officinaAnswerResult`, else a confirmed resume. With
 *  `__officinaAnswerHeld` set, the answer waits until the test calls
 *  `__officinaReleaseAnswer`, as a resume awaiting its confirmation does.
 *  A proposal sent by its id (AD-182) and not refused is accepted: it
 *  leaves the meetings served. */
function answer(proposalId: string | null): Promise<AnswerResult> {
  const result = (): AnswerResult => {
    const settled = window.__officinaAnswerResult ?? { outcome: "confirmed" };
    if (proposalId !== null && settled.outcome !== "refused") withoutProposal(proposalId);
    return settled;
  };
  if (!window.__officinaAnswerHeld) return Promise.resolve(result());
  return new Promise((resolve) => {
    window.__officinaReleaseAnswer = () => resolve(result());
  });
}

/** The meetings served, the answer proposal `id` gone from them; true
 *  when one showed it. */
function withoutProposal(id: string): boolean {
  const meetings = window.__officinaMeetings;
  if (!meetings) return false;
  const shown = Object.values(meetings).some((m) => m.proposed_answer?.id === id);
  window.__officinaMeetings = Object.fromEntries(
    Object.entries(meetings).map(([key, m]) => [key, m.proposed_answer?.id === id ? { ...m, proposed_answer: null } : m]),
  );
  return shown;
}

/** Mirrors `open_meeting` (AD-84): the meeting the test set for the key,
 *  else the backend's refusal of a key it does not know. */
function meetingOf(key: string): Promise<MeetingView> {
  const view = window.__officinaMeetings?.[key];
  if (view) return Promise.resolve(view);
  const error: IpcError = { code: "refused", message: `${key} is no longer on the board, and the registry does not hold it`, refusal: null };
  return Promise.reject(error);
}

/** Mirrors the collector's answer to an author request: a refusal names
 *  its rule and writes nothing; an accepted `create_subject` answers an id. */
function author(cmd: string): Promise<number | null> {
  const refusal = window.__officinaRefuse?.[cmd];
  if (refusal) {
    const error: IpcError = { code: "author_refused", message: `refused: ${refusal}`, refusal };
    return Promise.reject(error);
  }
  return Promise.resolve(cmd === "create_subject" ? 1 : null);
}

/** Mirrors the collector's answer to `issue_card` and `edit_card` (AD-78):
 *  the card or refusal the test sets in `__officinaCardAnswer`, else the
 *  refusal of a catalogue with no template approved. */
function card(): CardAnswer {
  return window.__officinaCardAnswer ?? { outcome: "refused", value: { kind: "template", detail: "not_approved" } };
}

/** Mirrors the collector's answer to `set_work_order_permission` (story
 *  5.24, D-985): the card the test set, issued again under a new id with
 *  `permission`, `--permission-mode auto` before the separator for
 *  Officina's default; the card it answers is the next issue's too. */
function switched(permission: WorkOrderPermission): CardAnswer {
  const current = card();
  if (current.outcome !== "issued") return current;
  const argv = current.value.argv.filter((arg, i, all) => arg !== "--permission-mode" && all[i - 1] !== "--permission-mode");
  if (permission === "officina") argv.splice(argv.indexOf("--"), 0, "--permission-mode", "auto");
  const answer: CardAnswer = {
    outcome: "issued",
    value: { ...current.value, id: `${current.value.id.slice(0, 31)}${permission === "officina" ? "1" : "0"}`, permission, argv },
  };
  window.__officinaCardAnswer = answer;
  return answer;
}

/** Mirrors the collector's answer to `dismiss_proposal` (FR-12): a
 *  pending or invalid proposal of the view is dismissed and the next view
 *  shows it decided; anything else is refused. */
async function dismissProposal(id: string): Promise<void> {
  const inMeeting = withoutProposal(id);
  const current = window.__officinaOrchestrator;
  const found = current?.proposals.find((p) => p.id === id);
  if (inMeeting && !found) return;
  if (!current || !found || (found.status !== "pending" && found.status !== "invalid")) {
    return refused("the proposal is neither pending nor invalid: the next view shows where it stands");
  }
  await window.__officinaPushOrchestrator?.({
    ...current,
    proposals: current.proposals.map((p) => (p.id === id ? { ...p, status: "dismissed" } : p)),
  });
}

/** Mirrors the collector's answer to `launch_work_order` (AD-81): the
 *  outcome or refusal the test sets in `__officinaLaunchAnswer`, else
 *  `started`. With `__officinaLaunchHeld` set, the answer waits until the
 *  test calls `__officinaReleaseLaunch`, as a spawn in progress does. */
function launch(): Promise<LaunchAnswer> {
  const answer = (): LaunchAnswer => window.__officinaLaunchAnswer ?? { outcome: "launched", value: "started" };
  if (!window.__officinaLaunchHeld) return Promise.resolve(answer());
  return new Promise((resolve) => {
    window.__officinaReleaseLaunch = () => resolve(answer());
  });
}

/** Mirrors the answer to `build_images` (AD-99): the answer the test
 *  sets in `__officinaBuildAnswer`, else `queued`. */
function buildImages(): Promise<BuildAnswer> {
  return Promise.resolve(window.__officinaBuildAnswer ?? { outcome: "queued" });
}

/** Mirrors the answer to `run_battery` (AD-117): the answer the test
 *  sets in `__officinaBatteryAnswer`, else the base image queued. */
function runBattery(): Promise<BatteryAnswer> {
  return Promise.resolve(
    window.__officinaBatteryAnswer ?? { outcome: "queued", detail: ["officina-workshop:local"] },
  );
}

/** Mirrors the answer to `open_review` (AD-121): the view the test sets
 *  in `__officinaReviews` for that run, else a `no_change` view of it. */
function openReview(runId: string): Promise<ReviewView> {
  const set = window.__officinaReviews?.[runId];
  if (set) return Promise.resolve(set);
  const base = "0".repeat(40);
  return Promise.resolve({
    run_id: runId,
    template: "bump",
    project: "api",
    base_branch: "main",
    base_commit: base,
    tip: base,
    state: "no_change",
    outcome: "no_change",
    cause: "exited",
    quarantine_cause: null,
    nothing_to_review: true,
    report_tail: null,
    facts: {
      duration_ms: null,
      cost_usd_micros: null,
      turns: null,
      exit_code: 0,
      hooks: {},
      allowed: [],
      refused: 0,
      refusals: [],
      clock_skew_ms: null,
      commits_past_base: 0,
      files_changed: null,
      insertions: null,
      deletions: null,
      collect_failed: null,
      credential_locations: null,
      credential_places: [],
    },
    commits: [],
    files: [],
    diff: null,
    groups: [],
    readiness: { ready: false, reasons: [{ cause: "not_in_review", group: null }] },
  });
}

/** Mirrors the answer to `approve_sensitive` (AD-113): the answer the
 *  test sets in `__officinaApprovalAnswer`, else `approved`. */
function approveSensitive(): Promise<ApprovalAnswer> {
  return Promise.resolve(window.__officinaApprovalAnswer ?? { outcome: "approved" });
}

/** Mirrors the answer to `import_package` (AD-114): the answer the test
 *  sets in `__officinaImportAnswer`, else `imported` on the run's own
 *  branch name. Every call is counted in `__officinaImportCalls`, so a
 *  keyboard Import can be shown to make none (D-225). */
function importPackage(runId: string): Promise<ImportAnswer> {
  window.__officinaImportCalls = (window.__officinaImportCalls ?? 0) + 1;
  return Promise.resolve(
    window.__officinaImportAnswer ?? { outcome: "imported", detail: `officina/${runId}` },
  );
}

/** Mirrors the answer to `discard_package` (AD-114): the answer the test
 *  sets in `__officinaDiscardAnswer`, else `discarded`. */
function discardPackage(): Promise<DiscardAnswer> {
  return Promise.resolve(window.__officinaDiscardAnswer ?? { outcome: "discarded" });
}

/** Mirrors the answer to `stop_workshop` (AD-116): the answer the test
 *  sets in `__officinaStopAnswer`, else nothing stopped, within budget.
 *  Every call is counted in `__officinaStopCalls`. */
function stopWorkshop(): Promise<StopAnswer> {
  window.__officinaStopCalls = (window.__officinaStopCalls ?? 0) + 1;
  return Promise.resolve(
    window.__officinaStopAnswer ?? { containers: 0, runs: 0, engine_reached: true, elapsed_ms: 0 },
  );
}

/** Mirrors the answer to `resume_workshop` (AD-116): the choices counted
 *  as sent, the relay as asked; every call's arguments kept in
 *  `__officinaResumeCalls`, so a keyboard Resume can be shown to make
 *  none (D-28). */
function resumeWorkshop(choices: ResumeChoice[], resumeRelay: boolean): Promise<ResumeAnswer> {
  window.__officinaResumeCalls = [...(window.__officinaResumeCalls ?? []), { choices, resumeRelay }];
  const removed = choices.filter((c) => c.choice === "remove").length;
  return Promise.resolve({ collected: choices.length - removed, removed, relay_resumed: resumeRelay });
}

/** Mirrors the answer to `remove_run` (AD-121): the answer the test sets
 *  in `__officinaRemoveRunAnswer`, else two objects removed. */
function removeRun(): Promise<RemoveRunAnswer> {
  return Promise.resolve(window.__officinaRemoveRunAnswer ?? { status: "removed", detail: 2 });
}

/** Mirrors the answer to `prepare_login` (D-605): the answer the test sets
 *  in `__officinaPrepareLoginAnswer`, else a ready command. */
function prepareLogin(): Promise<PrepareLoginAnswer> {
  return Promise.resolve(
    window.__officinaPrepareLoginAnswer ?? {
      status: "ready",
      detail: "docker run --rm -it officina-workshop:local auth login",
    },
  );
}

/** Mirrors the answer to `check_login` (D-608): the answer the test sets in
 *  `__officinaCheckLoginAnswer`, else a logged-in login still unchecked. */
function checkLogin(): Promise<CheckLoginAnswer> {
  return Promise.resolve(
    window.__officinaCheckLoginAnswer ?? {
      status: "checked",
      detail: { logged_in: true, auth_method: "claude.ai", unreadable: false, state: "unchecked" },
    },
  );
}

/** Mirrors the answer to `start_orchestrator` (AD-89): the answer the
 *  test sets in `__officinaStartAnswer`, else `started`. With
 *  `__officinaStartHeld` set, it waits until the test calls
 *  `__officinaReleaseStart`, as a spawn in progress does. */
function startOrchestrator(): Promise<StartAnswer> {
  const answer = (): StartAnswer => window.__officinaStartAnswer ?? { outcome: "started" };
  if (!window.__officinaStartHeld) return Promise.resolve(answer());
  return new Promise((resolve) => {
    window.__officinaReleaseStart = () => resolve(answer());
  });
}

/** Mirrors the collector's answer to `confirm_triage` (AD-49): each item is
 *  answered on its own, as the test sets it in `__officinaConfirmOutcome`
 *  by key, else confirmed with the verdict sent; a test shows the outcome
 *  on screen by pushing the next registry view. */
function confirmTriage(items: ConfirmItem[]): ItemAnswer[] {
  return items.map((item) => ({
    key: item.key,
    outcome: window.__officinaConfirmOutcome?.[item.key] ?? { kind: "confirmed", subject: 0, verdict: item.verdict },
  }));
}

/** Mirrors the collector's answer to `evaluate_triage_model` (D-92): the
 *  number of cases once queued, or the refusal the test sets; a test shows
 *  the progress by pushing the next registry view. */
function evaluate(): Promise<number> {
  const refused = window.__officinaEvaluateRefused;
  if (refused) {
    const error: IpcError = { code: "refused", message: refused, refusal: null };
    return Promise.reject(error);
  }
  return Promise.resolve(window.__officinaRegistry?.model.latest_dry_run?.tally.cases ?? 1);
}

/** Mirrors the collector's answer to `set_auto_archive` (FR-12): turning
 *  it on is refused unless the view's gate holds; the next view shows the
 *  switch. */
async function setAutoArchive(on: boolean): Promise<void> {
  const current = window.__officinaRegistry;
  if (!current) return;
  const gate = current.gates.auto_archive.state;
  if (on && gate !== "holds") {
    const error: IpcError = {
      code: "refused",
      message: `auto-archive cannot be turned on: its gate does not hold (${gate})`,
      refusal: null,
    };
    return Promise.reject(error);
  }
  await window.__officinaPushRegistry?.({
    ...current,
    auto_archive: { on, src: "author", at_ms: current.generated_at_ms, reason: null },
  });
}

/** Mirrors the collector's answer to `set_triage_model` (FR-10 as
 *  amended, D-81): turning it on is refused while the model cannot be
 *  called or its gate does not hold, and lifts a pause; the next view shows
 *  the switch. */
async function setTriageModel(on: boolean): Promise<void> {
  const current = window.__officinaRegistry;
  if (!current) return;
  const refused = (message: string): Promise<void> => {
    const error: IpcError = { code: "refused", message, refusal: null };
    return Promise.reject(error);
  };
  if (on && current.model.unavailable !== null) {
    return refused(`the triage model cannot be called: ${current.model.unavailable}`);
  }
  const gate = current.gates.model_enable.state;
  if (on && gate !== "holds") {
    return refused(`the triage model cannot be turned on: its gate does not hold (${gate})`);
  }
  await window.__officinaPushRegistry?.({
    ...current,
    model: {
      ...current.model,
      enabled: on,
      src: "author",
      at_ms: current.generated_at_ms,
      reason: null,
      paused_at_ms: on ? null : current.model.paused_at_ms,
    },
  });
}

/** Mirrors `authority::approve_catalogue`: approves only the hash the
 *  backend still holds as pending; otherwise the collector shows the current
 *  view and the call is refused. A test sets the backend's state with
 *  `__officinaRelay` (without an event, as a floor edit the window has not
 *  seen yet) and the view after a success with `__officinaRelayApproved`. */
async function approve(hash: string): Promise<string> {
  const current = window.__officinaRelay ?? null;
  if (current?.pending?.hash !== hash) {
    if (current) await window.__officinaPushRelay?.(current);
    const error: IpcError = {
      code: "catalogue_changed",
      message: `officina-window-authority: the catalogue changed since it was displayed (displayed ${hash}, current ${current?.pending?.hash ?? "none"}); review the new diff`,
      refusal: null,
    };
    return Promise.reject(error);
  }
  const approved = window.__officinaRelayApproved;
  if (approved) await window.__officinaPushRelay?.(approved);
  return hash;
}

/** Mirrors `authority::set_relay_stopped`: the flag and its time change,
 *  nothing else the view shows. */
async function setStopped(stopped: boolean): Promise<boolean> {
  const current = window.__officinaRelay;
  if (!current || current.stopped === stopped) return false;
  await window.__officinaPushRelay?.({
    ...current,
    stopped,
    stopped_since_ms: stopped ? Date.now() : null,
  });
  return true;
}

/** A refusal of the registry's writer, as the window's commands return it. */
function refused(message: string): Promise<void> {
  const error: IpcError = { code: "refused", message, refusal: null };
  return Promise.reject(error);
}

/** Mirrors the collector's answer to `set_auto_approval` (AD-88): on is
 *  refused unless the view's gate holds; the next view shows the switch. */
async function setAutoApproval(on: boolean): Promise<void> {
  const current = window.__officinaRelay;
  if (!current) return;
  if (on && !current.auto.stats.gate) {
    return refused(
      `auto approval needs at least ${current.auto.stats.gate_cases} shadow reviews met by your decision in the last 30 days, with no disagreement`,
    );
  }
  await window.__officinaPushRelay?.({
    ...current,
    auto: { ...current.auto, approval_on: on, approval_src: "author", approval_reason: null },
  });
}

/** Mirrors the collector's answer to `set_auto_mode` (D-162): `rule` is
 *  refused unless the view's rule offer holds. */
async function setAutoMode(mode: AutoMode): Promise<void> {
  const current = window.__officinaRelay;
  if (!current) return;
  if (mode === "rule" && !current.auto.stats.rule_offer) {
    return refused(
      `rule mode needs the approval gate and review and rule differing in under ${current.auto.stats.rule_diff_percent} % of the cases`,
    );
  }
  await window.__officinaPushRelay?.({ ...current, auto: { ...current.auto, mode } });
}

/** Mirrors the collector's answer to `mark_should_not_have_run` (AD-88):
 *  the run is marked, approval turns off with the reason, the mode returns
 *  to `review`, and the mark is a disagreement that breaks the gate. */
async function markRun(runId: string): Promise<void> {
  const current = window.__officinaRelay;
  if (!current) return;
  const run = current.auto.runs.find((r) => r.run_id === runId);
  if (!run || run.marked_at_ms !== null) return refused(`run ${runId} is no run a review or the rule approved in the last 7 days`);
  const stats = current.auto.stats;
  await window.__officinaPushRelay?.({
    ...current,
    auto: {
      ...current.auto,
      approval_on: false,
      approval_src: "author",
      approval_reason: "should_not_have_run",
      mode: "review",
      stats: { ...stats, disagreements: stats.disagreements + 1, gate: false, rule_offer: false },
      runs: current.auto.runs.map((r) => (r.run_id === runId ? { ...r, marked_at_ms: Date.now() } : r)),
    },
  });
}

/** Mirrors `authority::acknowledge_anomaly`: the anomaly stays listed,
 *  acknowledged. */
async function acknowledge(key: string): Promise<void> {
  const current = window.__officinaRelay;
  if (!current) return;
  await window.__officinaPushRelay?.({
    ...current,
    anomalies: current.anomalies.map((a) => (a.key === key ? { ...a, acknowledged: true } : a)),
  });
}

/** Mirrors `authority::decide_approval`: a request the board lists as valid
 *  and pending is decided and leaves Needs you with the next board;
 *  anything else is refused, as the backend refuses an unknown, invalid or
 *  reviewing request, and a reason that does not fit its class. */
async function decide(id: string, approve: boolean, reason?: string): Promise<void> {
  const board = window.__officinaBoard;
  const approval = board?.approvals.find((a) => a.id === id);
  const refuse = (why: string): Promise<void> => {
    const error: IpcError = { code: "refused", message: `officina-window-authority: request ${id} ${why}`, refusal: null };
    return Promise.reject(error);
  };
  if (!board || !approval?.valid) return refuse("is invalid or unknown");
  if (approval.state !== "pending") return refuse("is being reviewed");
  const needsReason = !approve && approval.class === "auto";
  if (needsReason !== (reason !== undefined)) return refuse("has a reason that does not fit its class");
  await window.__officinaPushBoard?.({
    ...board,
    approvals: board.approvals.filter((a) => a.id !== id),
    waiting_count: board.waiting_count - 1,
  });
}
