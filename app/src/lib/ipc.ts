// The only module that talks to the Rust side (AD-20). Components import
// these functions, never `@tauri-apps/api` directly, so the e2e mock and the
// real backend are interchangeable.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AuthorReason } from "./generated/AuthorReason";
import type { AutoMode } from "./generated/AutoMode";
import type { BatteryAnswer } from "./generated/BatteryAnswer";
import type { Board } from "./generated/Board";
import type { ApprovalAnswer } from "./generated/ApprovalAnswer";
import type { DiscardAnswer } from "./generated/DiscardAnswer";
import type { DiscardReason } from "./generated/DiscardReason";
import type { ImportAnswer } from "./generated/ImportAnswer";
import type { RemoveRunAnswer } from "./generated/RemoveRunAnswer";
import type { ResumeAnswer } from "./generated/ResumeAnswer";
import type { ResumeChoice } from "./generated/ResumeChoice";
import type { StopAnswer } from "./generated/StopAnswer";
import type { BuildAnswer } from "./generated/BuildAnswer";
import type { CardAnswer } from "./generated/CardAnswer";
import type { CardChange } from "./generated/CardChange";
import type { CardRequest } from "./generated/CardRequest";
import type { ConfirmItem } from "./generated/ConfirmItem";
import type { DiscardedSession } from "./generated/DiscardedSession";
import type { FocusOutcome } from "./generated/FocusOutcome";
import type { IpcError } from "./generated/IpcError";
import type { ItemAnswer } from "./generated/ItemAnswer";
import type { LaunchAnswer } from "./generated/LaunchAnswer";
import type { MeetingView } from "./generated/MeetingView";
import type { AnswerResult } from "./generated/AnswerResult";
import type { Delivery } from "./generated/Delivery";
import type { OrchestratorView } from "./generated/OrchestratorView";
import type { RegistryView } from "./generated/RegistryView";
import type { RelayView } from "./generated/RelayView";
import type { ReviewView } from "./generated/ReviewView";
import type { StartAnswer } from "./generated/StartAnswer";
import type { Settlement } from "./generated/Settlement";
import type { SubjectOrigin } from "./generated/SubjectOrigin";
import type { SubjectView } from "./generated/SubjectView";
import type { WorkOrderPermission } from "./generated/WorkOrderPermission";

export const BOARD_EVENT = "board://updated";

/** True when `error` is the `IpcError` every failing command rejects with. */
export function isIpcError(error: unknown): error is IpcError {
  return (
    typeof error === "object" &&
    error !== null &&
    typeof (error as { code?: unknown }).code === "string" &&
    typeof (error as { message?: unknown }).message === "string"
  );
}

/** What a rejected command says: an `IpcError`'s message; anything else,
 *  such as a fault of the IPC layer itself, as text. */
export function errorMessage(error: unknown): string {
  return isIpcError(error) ? error.message : String(error);
}
export const RELAY_EVENT = "relay://updated";

export function getRelay(): Promise<RelayView | null> {
  return invoke<RelayView | null>("get_relay");
}

export function onRelay(handler: (relay: RelayView) => void): Promise<UnlistenFn> {
  return listen<RelayView>(RELAY_EVENT, (event) => handler(event.payload));
}

/** Approves the catalogue whose hash the window displayed (AD-24); the
 *  backend refuses when the floor changed since. Returns the approved hash. */
export function approveCatalogue(hash: string): Promise<string> {
  return invoke<string>("approve_catalogue", { hash });
}

/** Approves or rejects a `human` or `auto` action's request (AD-35, AD-86);
 *  rejecting an `auto` one takes its reason. The backend refuses one that is
 *  unknown, decided, expired, being reviewed or invalid (D-27, D-157), and a
 *  reason that does not fit its class. */
export function decideApproval(id: string, approve: boolean, reason?: AuthorReason): Promise<void> {
  return invoke("decide_approval", reason ? { id, approve, reason } : { id, approve });
}

/** Acknowledges an anomaly by its key (D-173): the Relay view keeps it,
 *  marked acknowledged, and doctor no longer fails on it. */
export function acknowledgeAnomaly(key: string): Promise<void> {
  return invoke("acknowledge_anomaly", { key });
}

/** Turns auto approval on, refused unless the shadow gate holds when the
 *  registry writes, or off (AD-88). The caller sends `true` on a pointer
 *  click only (D-28). */
export function setAutoApproval(on: boolean): Promise<void> {
  return invoke("set_auto_approval", { on });
}

/** Sets auto approval's mode (D-162): `rule` refused unless the rule offer
 *  holds when the registry writes, `review` always. */
export function setAutoMode(mode: AutoMode): Promise<void> {
  return invoke("set_auto_mode", { mode });
}

/** Marks an auto-approved run "should not have run" (AD-88, FR-9): auto
 *  approval turns off and the mode returns to `review`. Sent on a pointer
 *  click only (D-28). */
export function markShouldNotHaveRun(runId: string): Promise<void> {
  return invoke("mark_should_not_have_run", { runId });
}

/** Stops or resumes the relay (AD-34). True when the state changed. */
export function setRelayStopped(stopped: boolean): Promise<boolean> {
  return invoke<boolean>("set_relay_stopped", { stopped });
}

export const REGISTRY_EVENT = "registry://updated";

/** The last registry view, for the first paint; later ones come as events. */
export function getRegistry(): Promise<RegistryView | null> {
  return invoke<RegistryView | null>("get_registry");
}

export function onRegistry(handler: (view: RegistryView) => void): Promise<UnlistenFn> {
  return listen<RegistryView>(REGISTRY_EVENT, (event) => handler(event.payload));
}

/** The archived subjects the author may reopen (AD-54). */
export function listArchived(): Promise<SubjectView[]> {
  return invoke<SubjectView[]>("list_archived");
}

/** Creates a subject (D-95): from selected sessions, which give it their
 *  project and are filed under it, or in a project the registry knows.
 *  Returns its id. */
export function createSubject(name: string, origin: SubjectOrigin): Promise<number> {
  return invoke<number>("create_subject", { name, origin });
}

export function renameSubject(subject: number, name: string): Promise<void> {
  return invoke("rename_subject", { subject, name });
}

/** Moves `source`'s sessions to `target` and archives `source` (D-55). */
export function mergeSubjects(source: number, target: number): Promise<void> {
  return invoke("merge_subjects", { source, target });
}

/** Files `sessions` under `subject`, or unfiles them with `null` (D-56). */
export function fileSessions(sessions: string[], subject: number | null): Promise<void> {
  return invoke("file_sessions", { sessions, subject });
}

/** Archives `subject`; `routine` when it could have run alone (FR-13). */
export function archiveSubject(subject: number, routine: boolean): Promise<void> {
  return invoke("archive_subject", { subject, routine });
}

/** Reopens an archived subject as `to_resume` (AD-48). */
export function reopenSubject(subject: number): Promise<void> {
  return invoke("reopen_subject", { subject });
}

/** Records the author's verdict on one ended session from the board, or
 *  takes it back with `null` (story 4.15): the next board shows it. */
export function settleSession(session: string, settlement: Settlement | null): Promise<void> {
  return invoke("settle_session", { session, settlement });
}

/** The sessions the author discarded from the board, the latest first. */
export function listDiscarded(): Promise<DiscardedSession[]> {
  return invoke<DiscardedSession[]>("list_discarded");
}

/** Confirms triage rows in one batch (AD-49): each item carries the row's
 *  key and the proposal shown; each is answered on its own. */
export function confirmTriage(items: ConfirmItem[]): Promise<ItemAnswer[]> {
  return invoke<ItemAnswer[]>("confirm_triage", { items });
}

/** Queues a dry run of the disabled triage model on past confirmations
 *  (AD-52): answered once queued, with how many cases it holds; its
 *  progress and result come in the registry view, never as a proposal. */
export function evaluateTriageModel(): Promise<number> {
  return invoke<number>("evaluate_triage_model");
}

/** Turns the triage model on, refused while it cannot be called or its
 *  enable gate does not hold, or off (FR-10 as amended); turning it on
 *  lifts a pause (D-81). */
export function setTriageModel(on: boolean): Promise<void> {
  return invoke("set_triage_model", { on });
}

/** Turns auto-archive on, refused unless its gate holds, or off (FR-12). */
export function setAutoArchive(on: boolean): Promise<void> {
  return invoke("set_auto_archive", { on });
}

/** Issues a work order card (AD-78): the card, its argument vector exactly
 *  as it would run, or the refusal. */
export function issueCard(request: CardRequest): Promise<CardAnswer> {
  return invoke<CardAnswer>("issue_card", { request });
}

/** A new card in place of `cardId` with one change (AD-78); the old card
 *  is superseded. */
export function editCard(cardId: string, change: CardChange): Promise<CardAnswer> {
  return invoke<CardAnswer>("edit_card", { cardId, change });
}

/** Switches the work orders of card `cardId`'s project to `permission`
 *  (story 5.24, D-985): the card issued again with it, or why not. The
 *  caller sends `officina` on a pointer click only (D-986). */
export function setWorkOrderPermission(cardId: string, permission: WorkOrderPermission): Promise<CardAnswer> {
  return invoke<CardAnswer>("set_work_order_permission", { cardId, permission });
}

/** Issues a card from the pending work order proposal `proposalId`
 *  (AD-91, D-152): its stored values through the author's validation, or
 *  why it opens none. */
export function issueCardFromProposal(proposalId: string): Promise<CardAnswer> {
  return invoke<CardAnswer>("issue_card_from_proposal", { proposalId });
}

/** Dismisses the pending or invalid proposal `proposalId` (FR-12). */
export function dismissProposal(proposalId: string): Promise<void> {
  return invoke("dismiss_proposal", { proposalId });
}

/** Launches card `cardId`, whose shown prompt hashes to `promptSha256`
 *  and, for a workshop card, whose shown plan hashes to `planSha256`
 *  (AD-78, AD-107): how it ended, or why it was refused. The caller sends
 *  it on a pointer click only (D-28). */
export function launchWorkOrder(
  cardId: string,
  promptSha256: string,
  planSha256: string | null,
): Promise<LaunchAnswer> {
  return invoke<LaunchAnswer>("launch_work_order", { cardId, promptSha256, planSha256 });
}

export const ORCHESTRATOR_EVENT = "orchestrator://updated";

/** The last Orchestrator view, for the first paint; later ones come as
 *  events (AD-92). */
export function getOrchestrator(): Promise<OrchestratorView | null> {
  return invoke<OrchestratorView | null>("get_orchestrator");
}

export function onOrchestrator(handler: (view: OrchestratorView) => void): Promise<UnlistenFn> {
  return listen<OrchestratorView>(ORCHESTRATOR_EVENT, (event) => handler(event.payload));
}

/** Records that the author read the briefing, on leaving the Orchestrator
 *  view (AD-83): the next briefing counts its changes from now. */
export function markBriefingRead(): Promise<void> {
  return invoke("mark_briefing_read");
}

/** Starts the orchestrator session in a console of its own, or brings the
 *  live one forward (AD-89): how the Start was answered. */
export function startOrchestrator(): Promise<StartAnswer> {
  return invoke<StartAnswer>("start_orchestrator");
}

/** Build images (AD-99): the gateway, the base and the project images,
 *  on a pointer click only (D-28); whether the builds were queued or why
 *  not. Each image's outcome reaches the workshop view as it ends. */
export function buildImages(): Promise<BuildAnswer> {
  return invoke<BuildAnswer>("build_images");
}

/** Run escape battery (AD-117): on `image`, or every image whose battery
 *  is stale when `image` is null, on a pointer click only (D-28); which
 *  batteries were queued or why none. Each one's outcome reaches the
 *  workshop view as it ends. */
export function runBattery(image: string | null): Promise<BatteryAnswer> {
  return invoke<BatteryAnswer>("run_battery", { image });
}

/** Receive on workshop run `runId`'s review (AD-121, D-239): the stored
 *  facts, then host git's commits, files and diff; a `no_change` run
 *  answers `nothing_to_review`. The first Receive of a package journals
 *  `review_opened`. */
export function openReview(runId: string): Promise<ReviewView> {
  return invoke<ReviewView>("open_review", { runId });
}

/** Approve on group `groupKey` of run `runId`'s package (AD-113), pointer
 *  only: `hunksSha256` is the hash the panel showed, and a group whose
 *  hunks hash otherwise now is refused `hunks_changed`. */
export function approveSensitive(
  runId: string,
  groupKey: string,
  hunksSha256: string,
): Promise<ApprovalAnswer> {
  return invoke<ApprovalAnswer>("approve_sensitive", { runId, groupKey, hunksSha256 });
}

/** Import on run `runId`'s package (AD-114), pointer only: `tip` is the
 *  tip the review showed. The answer names the new local branch, or why
 *  Import was refused; nothing is merged or pushed. */
export function importPackage(runId: string, tip: string): Promise<ImportAnswer> {
  return invoke<ImportAnswer>("import_package", { runId, tip });
}

/** Discard on run `runId`'s package with one of the author's reasons
 *  (AD-114); `expired` and `not_collected` are refused, being the
 *  system's. */
export function discardPackage(runId: string, reason: DiscardReason): Promise<DiscardAnswer> {
  return invoke<DiscardAnswer>("discard_package", { runId, reason });
}

/** Stop all (AD-116): every worker and gateway of this floor stopped, the
 *  relay too, and nothing launched until Resume; it only reduces, so the
 *  keyboard reaches it. */
export function stopWorkshop(): Promise<StopAnswer> {
  return invoke<StopAnswer>("stop_workshop");
}

/** Resume (AD-116), pointer only: each kept run to collect (the default)
 *  or remove, and the relay resumed too when `resumeRelay`. */
export function resumeWorkshop(choices: ResumeChoice[], resumeRelay: boolean): Promise<ResumeAnswer> {
  return invoke<ResumeAnswer>("resume_workshop", { choices, resumeRelay });
}

/** Removes a kept or orphan run's objects of this floor (AD-121); it only
 *  reduces, so the keyboard reaches it. */
export function removeRun(runId: string): Promise<RemoveRunAnswer> {
  return invoke<RemoveRunAnswer>("remove_run", { runId });
}

/** The meeting of `key` (AD-84, D-138): a card key, `sid:<session id>`
 *  or `subject:<id>`; read on Receive, and again when the card's state
 *  changes. */
export function openMeeting(key: string): Promise<MeetingView> {
  return invoke<MeetingView>("open_meeting", { key });
}

/** Sends the author's answer to the meeting of `key` (AD-85), the
 *  delivery the panel showed: the collector refuses it when the session's
 *  delivery changed. Waits for a resume's confirmation, up to 30 s. */
export function sendAnswer(key: string, expected: Delivery, text: string): Promise<AnswerResult> {
  return invoke<AnswerResult>("send_answer", { key, expected, text });
}

/** Sends the orchestrator's answer proposal `proposalId` to the meeting
 *  of `key` (AD-182, D-575): its id only, never a text, so what runs is
 *  the text the collector stored. The caller sends it on a pointer click
 *  only (D-28). */
export function sendProposedAnswer(key: string, expected: Delivery, proposalId: string): Promise<AnswerResult> {
  return invoke<AnswerResult>("send_answer", { key, expected, proposalId });
}

export function getBoard(): Promise<Board | null> {
  return invoke<Board | null>("get_board");
}

export function onBoard(handler: (board: Board) => void): Promise<UnlistenFn> {
  return listen<Board>(BOARD_EVENT, (event) => handler(event.payload));
}

export function refresh(): Promise<void> {
  return invoke("refresh");
}

export function focus(key: string): Promise<FocusOutcome> {
  return invoke<FocusOutcome>("focus", { key });
}

export function reportWrongWindow(key: string): Promise<void> {
  return invoke("report_wrong_window", { key });
}

export function attachCommand(key: string): Promise<string | null> {
  return invoke<string | null>("copy_attach_command", { key });
}

export async function setAlwaysOnTop(onTop: boolean): Promise<void> {
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  await getCurrentWindow().setAlwaysOnTop(onTop);
}
