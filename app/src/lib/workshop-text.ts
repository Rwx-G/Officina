// The window's words for the workshop card, the review panel and the
// Workshop section (AD-122, FR-6, FR-10 to FR-12, FR-17). Each table is keyed by its generated union, so a
// value the backend adds fails `pnpm check` until it has its text.
import type { ApprovalRefusal } from "./generated/ApprovalRefusal";
import type { BaseChoice } from "./generated/BaseChoice";
import type { BatteryOutcome } from "./generated/BatteryOutcome";
import type { BatteryRefusal } from "./generated/BatteryRefusal";
import type { BuildRefusal } from "./generated/BuildRefusal";
import type { CredentialKind } from "./generated/CredentialKind";
import type { CredentialState } from "./generated/CredentialState";
import type { EngineStatus } from "./generated/EngineStatus";
import type { ImageKind } from "./generated/ImageKind";
import type { ImageMismatch } from "./generated/ImageMismatch";
import type { ImageState } from "./generated/ImageState";
import type { LoginRefusal } from "./generated/LoginRefusal";
import type { ProxySource } from "./generated/ProxySource";
import type { RemoveRunRefusal } from "./generated/RemoveRunRefusal";
import type { DiscardReason } from "./generated/DiscardReason";
import type { DiscardRefusal } from "./generated/DiscardRefusal";
import type { ImportRefusal } from "./generated/ImportRefusal";
import type { MountState } from "./generated/MountState";
import type { NotReadyCause } from "./generated/NotReadyCause";
import type { QuarantineCause } from "./generated/QuarantineCause";
import type { SensitiveGroup } from "./generated/SensitiveGroup";
import type { WorkshopRunState } from "./generated/WorkshopRunState";

/** The three reasons the author gives a Discard; the other two are the
 *  system's, which `discard_package` refuses (AD-114). */
export const AUTHOR_DISCARD_REASONS = ["should_not_have_run", "poor_result", "not_needed"] as const satisfies readonly DiscardReason[];

export const DISCARD_REASON_TEXT: Record<DiscardReason, string> = {
  should_not_have_run: "Should not have run",
  poor_result: "Poor result",
  not_needed: "Not needed",
  expired: "Expired",
  not_collected: "Not collected",
};

export const DISCARD_REFUSAL_TEXT: Record<DiscardRefusal, string> = {
  unknown_run: "Officina no longer knows this run.",
  no_package: "This run has no package left to discard.",
  system_reason: "That reason is the system's, not yours.",
};

export const IMPORT_REFUSAL_TEXT: Record<ImportRefusal, string> = {
  unknown_run: "Officina no longer knows this run.",
  not_in_review: "The package is no longer in review.",
  unapproved: "A sensitive group is not approved yet.",
  state_changed: "The run changed state since the panel showed it: read it again.",
  branch_exists: "A branch of that name already exists in the project.",
  branch_conflict: "A branch of the project's conflicts with the one Import would create.",
  package_changed: "The package's tip is not the one the panel showed: read it again.",
  unavailable: "The project's repository could not be reached.",
};

export const APPROVAL_REFUSAL_TEXT: Record<ApprovalRefusal, string> = {
  unknown_run: "Officina no longer knows this run.",
  not_in_review: "The package is no longer in review.",
  unknown_group: "That group is not in this package.",
  hunks_changed: "The group's hunks changed since the panel showed them: read them again.",
  too_large: "The group is too large to approve: discard the package.",
  unavailable: "The package could not be read.",
};

export const NOT_READY_TEXT: Record<NotReadyCause, string> = {
  not_in_review: "The package is not in review.",
  unapproved: "Approve each sensitive group first.",
  too_large: "A sensitive group is too large to approve.",
};

export const QUARANTINE_TEXT: Record<QuarantineCause, string> = {
  branch_missing: "The work branch was missing from the bundle.",
  bundle_invalid: "Host git refused the bundle.",
  too_large: "The package is over its size bound.",
  wrong_base: "The work branch does not start from the run's base.",
  extra_ref: "The bundle carried a reference besides the work branch.",
  mount_committed: "A read-only mount's content was committed.",
  credential_found: "The credential scan found the worker's credential in the package.",
};

/** What the author does once a scan found the credential (AD-112). */
export const REVOKE_ADVICE =
  "Revoke the credential the floor stores and store a new one before the next launch: the worker could read the old one.";

export const MOUNT_STATE_TEXT: Record<MountState, string> = {
  ready: "read-only",
  missing: "missing",
  not_ignored: "not git-ignored",
  tracked: "tracked by git",
  path_unsupported: "path not supported",
  reparse_point: "a link, refused",
};

export const GROUP_TEXT: Record<SensitiveGroup, string> = {
  claude_config: "Claude Code configuration",
  git: "Git configuration",
  ci: "CI configuration",
  build_scripts: "Build scripts",
  toolchain_config: "Toolchain configuration",
  editor_shell: "Editor and shell files",
  officina: "Officina files",
  file_modes: "File modes",
  binary: "Binary files",
  project: "The project's sensitive paths",
  file: "Sensitive files",
};

/** A run's state as the board card's line says it. */
export const RUN_STATE_TEXT: Record<WorkshopRunState, string> = {
  launching: "launching",
  running: "running",
  stopping: "stopping",
  collecting: "collecting",
  review: "package in review",
  quarantined: "package quarantined",
  no_change: "no change",
  stopped: "kept by Stop all",
  imported: "imported",
  discarded: "discarded",
  failed: "failed",
  unknown: "unknown",
};

/** A base as its `<select>` option names it. */
export function baseText(base: BaseChoice): string {
  return "listed" in base ? base.listed.branch : `${base.imported.branch} (from run ${base.imported.parent_run_id.slice(0, 8)})`;
}

/** The branch of a base. */
export function baseBranch(base: BaseChoice): string {
  return "listed" in base ? base.listed.branch : base.imported.branch;
}

/** Two bases are one when their kind and branch agree. */
export function sameBase(a: BaseChoice, b: BaseChoice): boolean {
  return ("listed" in a) === ("listed" in b) && baseBranch(a) === baseBranch(b);
}

/** A budget in cents as dollars. */
export function dollars(cents: number): string {
  return `$${(cents / 100).toFixed(2)}`;
}

export const ENGINE_STATUS_TEXT: Record<EngineStatus, string> = {
  unresolved: "No Docker program found",
  unreachable: "Docker did not answer in time",
  unavailable: "Docker cannot run the workshop",
  ready: "Docker ready",
};

export const IMAGE_KIND_TEXT: Record<ImageKind, string> = {
  workshop: "base",
  browser: "browser base",
  gateway: "gateway",
  project: "project",
};

/** What Build images would do to an image in each state. */
export const IMAGE_STATE_TEXT: Record<ImageState, string> = {
  absent: "absent",
  building: "building",
  ready: "ready",
  mismatch: "out of date",
  failed: "build failed",
};

export const IMAGE_MISMATCH_TEXT: Record<ImageMismatch, string> = {
  absent: "not on the engine",
  not_recorded: "no build recorded",
  label_differs: "a label differs",
  base_id_differs: "built on another base",
  programs_differ: "built with other Linux programs",
};

export const BATTERY_OUTCOME_TEXT: Record<BatteryOutcome, string> = {
  passed: "passed",
  failed: "failed",
  stopped: "stopped",
  error: "could not run",
};

export const BUILD_REFUSAL_TEXT: Record<BuildRefusal, string> = {
  linux_programs_missing: "The Linux programs are not next to Officina.",
  workshop_stopped: "The workshop is stopped: resume it first.",
  workshop_off: "The floor declares no valid [workshop] table.",
  not_approved: "No catalogue is approved.",
  catalogue_changed: "The floor or a workshop file changed since its approval: approve it again.",
  claude_version_unknown: "The host's Claude Code version is not known.",
  engine_unavailable: "Docker cannot run the workshop.",
  builder_missing: "Docker has no builder for its context.",
  proxy_refused: "The installer host's route could not be decided: see officina doctor.",
  context_unwritable: "A build context could not be written under Officina's home.",
  busy: "A build or a battery is already running.",
};

export const BATTERY_REFUSAL_TEXT: Record<BatteryRefusal, string> = {
  workshop_stopped: "The workshop is stopped: resume it first.",
  workshop_off: "The floor declares no valid [workshop] table.",
  not_approved: "No catalogue is approved.",
  catalogue_changed: "The floor changed since its approval: approve it again.",
  image_missing: "An image has no successful build: build images first.",
  nothing_stale: "No battery is stale.",
  engine_unavailable: "Docker cannot run the workshop.",
  busy: "A build or a battery is already running.",
};

export const LOGIN_REFUSAL_TEXT: Record<LoginRefusal, string> = {
  workshop_stopped: "The workshop is stopped: resume it first.",
  no_floor: "No approved floor.",
  not_login_floor: "This floor does not use the login kind.",
  engine_unavailable: "Docker cannot run the workshop.",
  base_image_missing: "The base image is not built: build images first.",
};

export const REMOVE_REFUSAL_TEXT: Record<RemoveRunRefusal, string> = {
  malformed_id: "That is not a run id.",
  not_removable: "The run is neither kept nor an orphan.",
  no_floor: "No approved floor.",
  engine_unavailable: "Docker could not remove its objects.",
};

export const CREDENTIAL_KIND_TEXT: Record<CredentialKind, string> = {
  "oauth-token": "setup-token",
  login: "login",
  "api-key": "API key",
};

/** `ok` reads as `unchecked`: nothing sets it in v0.6 (D-1241). */
export const CREDENTIAL_STATE_TEXT: Record<CredentialState, string> = {
  unchecked: "unchecked",
  ok: "unchecked",
  missing: "missing",
  needs_login: "login needed",
  needs_new_credential: "a new credential needed",
};

export const PROXY_SOURCE_TEXT: Record<ProxySource, string> = {
  floor: "the floor's proxy",
  pac: "the PAC file Windows names",
  auto_detect: "Windows' automatic detection",
  static: "Windows' static proxy",
  none: "no proxy",
};

/** The live-run count's accessible name and tooltip (D-618). */
export function liveText(count: number): string {
  return `${count} in the workshop`;
}

/** Stop all's label, the same box whether in flight or not (D-517). */
export const STOP_ALL_TEXT = { ready: "Stop all", stopping: "Stopping" } as const;
