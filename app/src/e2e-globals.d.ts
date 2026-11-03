// Globals the e2e build exposes to Playwright (see src/e2e-mock.ts). Shared by
// the app and the e2e tests, which type-check under different tsconfigs.
import type { ApprovalAnswer } from "./lib/generated/ApprovalAnswer";
import type { DiscardAnswer } from "./lib/generated/DiscardAnswer";
import type { ImportAnswer } from "./lib/generated/ImportAnswer";
import type { BatteryAnswer } from "./lib/generated/BatteryAnswer";
import type { Board } from "./lib/generated/Board";
import type { BuildAnswer } from "./lib/generated/BuildAnswer";
import type { AuthorRefusal } from "./lib/generated/AuthorRefusal";
import type { CardAnswer } from "./lib/generated/CardAnswer";
import type { DiscardedSession } from "./lib/generated/DiscardedSession";
import type { FocusOutcome } from "./lib/generated/FocusOutcome";
import type { ItemOutcome } from "./lib/generated/ItemOutcome";
import type { LaunchAnswer } from "./lib/generated/LaunchAnswer";
import type { MeetingView } from "./lib/generated/MeetingView";
import type { AnswerResult } from "./lib/generated/AnswerResult";
import type { OrchestratorView } from "./lib/generated/OrchestratorView";
import type { RegistryView } from "./lib/generated/RegistryView";
import type { RelayView } from "./lib/generated/RelayView";
import type { ReviewView } from "./lib/generated/ReviewView";
import type { StartAnswer } from "./lib/generated/StartAnswer";
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
    /** What `list_discarded` answers (story 4.15). */
    __officinaDiscarded?: DiscardedSession[];
    /** Author commands the backend refuses, with the rule that refuses. */
    __officinaRefuse?: Record<string, AuthorRefusal>;
    /** Sets the registry view and emits `registry://updated`. */
    __officinaPushRegistry?: (view: RegistryView) => Promise<void>;
    /** What `confirm_triage` answers for an item, by row key (story 3.7);
     *  an item not named is confirmed. */
    __officinaConfirmOutcome?: Record<string, ItemOutcome>;
    /** The refusal `evaluate_triage_model` answers, when set (story 3.9). */
    __officinaEvaluateRefused?: string;
    /** What `issue_card` and `edit_card` answer (story 5.7). */
    __officinaCardAnswer?: CardAnswer;
    /** What `launch_work_order` answers (story 5.7). */
    __officinaLaunchAnswer?: LaunchAnswer;
    /** Holds `launch_work_order`'s answer until `__officinaReleaseLaunch`
     *  is called (story 5.9): a launch in flight. */
    __officinaLaunchHeld?: boolean;
    /** Sends the held launch's answer; set by the mock while one is held. */
    __officinaReleaseLaunch?: () => void;
    /** The Orchestrator view `get_orchestrator` answers (story 5.8). */
    __officinaOrchestrator?: OrchestratorView | null;
    /** What `open_meeting` answers, by key (story 5.10); a key not named
     *  is refused. */
    __officinaMeetings?: Record<string, MeetingView>;
    /** What `send_answer` answers (story 5.11); a confirmed resume when
     *  unset. */
    __officinaAnswerResult?: AnswerResult;
    /** Holds `send_answer`'s answer until `__officinaReleaseAnswer`. */
    __officinaAnswerHeld?: boolean;
    __officinaReleaseAnswer?: () => void;
    /** Sets the Orchestrator view and emits `orchestrator://updated`. */
    __officinaPushOrchestrator?: (view: OrchestratorView) => Promise<void>;
    /** What `start_orchestrator` answers (story 5.16). */
    __officinaStartAnswer?: StartAnswer;
    /** What `build_images` answers (story 6.8); else `queued`. */
    __officinaBuildAnswer?: BuildAnswer;
    /** What `run_battery` answers (story 6.10); else the base queued. */
    __officinaBatteryAnswer?: BatteryAnswer;
    /** What `open_review` answers, by run id (story 6.13); else a
     *  `no_change` view of that run. */
    __officinaReviews?: Record<string, ReviewView>;
    /** What `approve_sensitive` answers (story 6.15); else `approved`. */
    __officinaApprovalAnswer?: ApprovalAnswer;
    /** What `import_package` answers (story 6.16); else `imported`. */
    __officinaImportAnswer?: ImportAnswer;
    /** How many times `import_package` was called (story 6.16). */
    __officinaImportCalls?: number;
    /** What `discard_package` answers (story 6.16); else `discarded`. */
    __officinaDiscardAnswer?: DiscardAnswer;
    /** Holds `start_orchestrator`'s answer until `__officinaReleaseStart`
     *  is called (story 5.16): a Start in flight. */
    __officinaStartHeld?: boolean;
    /** Sends the held Start's answer; set by the mock while one is held. */
    __officinaReleaseStart?: () => void;
  }
}

export {};
