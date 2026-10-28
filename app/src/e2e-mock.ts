// Installed only in `--mode e2e` builds (AD-17): replaces the Rust backend
// with boards and relay views injected by Playwright, so the real UI runs in
// a browser.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import type { Board } from "./lib/generated/Board";
import type { RelayView } from "./lib/generated/RelayView";

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
          const { id } = args as { id: string; approve: boolean };
          return decide(id);
        }
        case "set_relay_stopped":
          return setStopped((args as { stopped: boolean }).stopped);
        default:
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
    return Promise.reject(
      `officina-window-authority: the catalogue changed since it was displayed (displayed ${hash}, current ${current?.pending?.hash ?? "none"}); review the new diff`,
    );
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

/** Mirrors `authority::decide_approval`: a request the board lists as valid
 *  is decided and leaves Needs you with the next board; anything else is
 *  refused, as the backend refuses an unknown or invalid request. */
async function decide(id: string): Promise<void> {
  const board = window.__officinaBoard;
  const approval = board?.approvals.find((a) => a.id === id);
  if (!board || !approval?.valid) {
    return Promise.reject(`officina-window-authority: request ${id} is invalid or unknown`);
  }
  await window.__officinaPushBoard?.({
    ...board,
    approvals: board.approvals.filter((a) => a.id !== id),
    waiting_count: board.waiting_count - 1,
  });
}
