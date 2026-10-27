// The only module that talks to the Rust side (AD-20). Components import
// these functions, never `@tauri-apps/api` directly, so the e2e mock and the
// real backend are interchangeable.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Board } from "./generated/Board";
import type { FocusOutcome } from "./generated/FocusOutcome";

export const BOARD_EVENT = "board://updated";

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
