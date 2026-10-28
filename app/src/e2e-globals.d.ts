// Globals the e2e build exposes to Playwright (see src/e2e-mock.ts). Shared by
// the app and the e2e tests, which type-check under different tsconfigs.
import type { Board } from "./lib/generated/Board";
import type { AuthorRefusal } from "./lib/generated/AuthorRefusal";
import type { FocusOutcome } from "./lib/generated/FocusOutcome";
import type { ItemOutcome } from "./lib/generated/ItemOutcome";
import type { RegistryView } from "./lib/generated/RegistryView";
import type { RelayView } from "./lib/generated/RelayView";
import type { SubjectView } from "./lib/generated/SubjectView";

declare global {
  interface Window {
    __officinaBoard?: Board | null;
    __officinaFocus?: Record<string, FocusOutcome>;
    __officinaCalls?: { cmd: string; args: unknown }[];
    __officinaPushBoard?: (board: Board) => Promise<void>;
    /** The relay view `get_relay` answers, and the backend's current state. */
    __officinaRelay?: RelayView | null;
    /** The view the backend shows after a successful `approve_catalogue`. */
    __officinaRelayApproved?: RelayView;
    __officinaPushRelay?: (view: RelayView) => Promise<void>;
    /** The registry view `get_registry` answers (story 3.5). */
    __officinaRegistry?: RegistryView | null;
    /** What `list_archived` answers. */
    __officinaArchived?: SubjectView[];
    /** Author commands the backend refuses, with the rule that refuses. */
    __officinaRefuse?: Record<string, AuthorRefusal>;
    /** Sets the registry view and emits `registry://updated`. */
    __officinaPushRegistry?: (view: RegistryView) => Promise<void>;
    /** What `confirm_triage` answers for an item, by row key (story 3.7);
     *  an item not named is confirmed. */
    __officinaConfirmOutcome?: Record<string, ItemOutcome>;
    /** The refusal `evaluate_triage_model` answers, when set (story 3.9). */
    __officinaEvaluateRefused?: string;
  }
}

export {};
