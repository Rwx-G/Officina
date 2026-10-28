// The only module that talks to the Rust side (AD-20). Components import
// these functions, never `@tauri-apps/api` directly, so the e2e mock and the
// real backend are interchangeable.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Board } from "./generated/Board";
import type { FocusOutcome } from "./generated/FocusOutcome";
import type { IpcError } from "./generated/IpcError";
import type { RegistryView } from "./generated/RegistryView";
import type { RelayView } from "./generated/RelayView";
import type { SubjectOrigin } from "./generated/SubjectOrigin";
import type { SubjectView } from "./generated/SubjectView";

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

/** Approves or rejects a `human` action's request (AD-35); the backend
 *  refuses one that is unknown, decided, expired or invalid (D-27). */
export function decideApproval(id: string, approve: boolean): Promise<void> {
  return invoke("decide_approval", { id, approve });
}

/** Acknowledges an anomaly by its key (D-173): the Relay view keeps it,
 *  marked acknowledged, and doctor no longer fails on it. */
export function acknowledgeAnomaly(key: string): Promise<void> {
  return invoke("acknowledge_anomaly", { key });
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
