// Installed only in `--mode e2e` builds (AD-17): replaces the Rust backend
// with boards and relay views injected by Playwright, so the real UI runs in
// a browser.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import type { Board } from "./lib/generated/Board";
import type { ConfirmItem } from "./lib/generated/ConfirmItem";
import type { IpcError } from "./lib/generated/IpcError";
import type { ItemAnswer } from "./lib/generated/ItemAnswer";
import type { RegistryView } from "./lib/generated/RegistryView";
import type { RelayView } from "./lib/generated/RelayView";

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
          const { id } = args as { id: string; approve: boolean };
          return decide(id);
        }
        case "set_relay_stopped":
          return setStopped((args as { stopped: boolean }).stopped);
        case "acknowledge_anomaly":
          return acknowledge((args as { key: string }).key);
        case "get_registry":
          return window.__officinaRegistry ?? null;
        case "list_archived":
          return window.__officinaArchived ?? [];
        case "confirm_triage":
          return confirmTriage((args as { items: ConfirmItem[] }).items);
        case "evaluate_triage_model":
          return evaluate();
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
 *  is decided and leaves Needs you with the next board; anything else is
 *  refused, as the backend refuses an unknown or invalid request. */
async function decide(id: string): Promise<void> {
  const board = window.__officinaBoard;
  const approval = board?.approvals.find((a) => a.id === id);
  if (!board || !approval?.valid) {
    const error: IpcError = {
      code: "refused",
      message: `officina-window-authority: request ${id} is invalid or unknown`,
      refusal: null,
    };
    return Promise.reject(error);
  }
  await window.__officinaPushBoard?.({
    ...board,
    approvals: board.approvals.filter((a) => a.id !== id),
    waiting_count: board.waiting_count - 1,
  });
}
