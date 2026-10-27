// Globals the e2e build exposes to Playwright (see src/e2e-mock.ts). Shared by
// the app and the e2e tests, which type-check under different tsconfigs.
import type { Board } from "./lib/generated/Board";
import type { FocusOutcome } from "./lib/generated/FocusOutcome";

declare global {
  interface Window {
    __officinaBoard?: Board | null;
    __officinaFocus?: Record<string, FocusOutcome>;
    __officinaCalls?: { cmd: string; args: unknown }[];
    __officinaPushBoard?: (board: Board) => Promise<void>;
  }
}

export {};
