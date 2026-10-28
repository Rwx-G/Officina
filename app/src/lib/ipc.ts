// The only module that talks to the Rust side (AD-20). Components import
// these functions, never `@tauri-apps/api` directly, so the e2e mock and the
// real backend are interchangeable.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Board } from "./generated/Board";
import type { FocusOutcome } from "./generated/FocusOutcome";
import type { RelayView } from "./generated/RelayView";

export const BOARD_EVENT = "board://updated";
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
