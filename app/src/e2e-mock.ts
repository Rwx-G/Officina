// Installed only in `--mode e2e` builds (AD-17): replaces the Rust backend
// with boards injected by Playwright, so the real UI runs in a browser.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import type { Board } from "./lib/generated/Board";
import type { FocusOutcome } from "./lib/generated/FocusOutcome";

export function installE2eMock(): void {
  window.__officinaCalls = [];
  mockWindows("main");
  mockIPC(
    (cmd, args) => {
      window.__officinaCalls?.push({ cmd, args });
      switch (cmd) {
        case "get_board":
          return window.__officinaBoard ?? null;
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
}
