// The relay's state in the window (AD-35): the last relay view, and the
// outcome of the author's approve and stop clicks.
import type { AutoMode } from "./generated/AutoMode";
import type { CanonicalAction } from "./generated/CanonicalAction";
import type { FieldChange } from "./generated/FieldChange";
import type { ModelAlias } from "./generated/ModelAlias";
import type { RelayView } from "./generated/RelayView";
import * as ipc from "./ipc";

/** An action's class as the catalogue shows it: the check it runs after,
 *  and an `auto` action's hourly cap (AD-76). */
export function classText(action: CanonicalAction): string {
  const terms: string[] = [];
  if (action.check) terms.push(`after ${action.check}`);
  if (action.auto_max_per_hour !== undefined) terms.push(`at most ${action.auto_max_per_hour} per hour`);
  return terms.length > 0 ? `${action.class} (${terms.join(", ")})` : action.class;
}

/** The review's model (AD-75): `null` records a failing `[auto_review]`
 *  table, which makes every `auto` request escalate. */
export function reviewModelText(model: ModelAlias | null): string {
  return model ?? "none, the [auto_review] table is invalid: every auto request escalates";
}

/** An approval the backend refused, kept to explain the banner. */
export interface RefusedApproval {
  /** The hash the author approved, as it was displayed. */
  hash: string;
  message: string;
}

/** True when the relay asks something of the author or holds something: a
 *  workstation without it shows no relay control at all. */
export function relayInUse(relay: RelayView | null): boolean {
  return (
    relay !== null &&
    (relay.approved_hash !== null || relay.pending !== null || relay.stopped || relay.snapshot_error !== null)
  );
}

/** A JSON value of a diff, as one line of text. */
export function shown(value: unknown): string {
  if (value === null || value === undefined) return "(none)";
  return typeof value === "string" ? value : JSON.stringify(value);
}

/** One line of a changed field, values already as text. */
export interface FieldLine {
  label: string;
  before: string;
  after: string;
}

function isMap(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** The lines of one changed field: a field holding a map (`params`, `env`)
 *  gives one line per key whose value changed, so a value added to one
 *  parameter is not lost in the whole map; any other field one line. */
export function fieldLines(change: FieldChange): FieldLine[] {
  const whole = [{ label: change.field, before: shown(change.before), after: shown(change.after) }];
  const { before, after } = change;
  if (!isMap(before) || !isMap(after)) return whole;
  const keys = Object.keys({ ...before, ...after }).sort();
  const lines = keys
    .filter((key) => JSON.stringify(before[key]) !== JSON.stringify(after[key]))
    .map((key) => ({ label: `${change.field}.${key}`, before: shown(before[key]), after: shown(after[key]) }));
  return lines.length > 0 ? lines : whole;
}

export class RelayStore {
  relay = $state<RelayView | null>(null);
  approving = $state(false);
  switching = $state(false);
  refused = $state<RefusedApproval | null>(null);
  stopError = $state<string | null>(null);
  acknowledging = $state<Record<string, boolean>>({});
  anomalyError = $state<string | null>(null);
  /** An `auto` switch or mark is on its way. */
  autoBusy = $state(false);
  /** Why the last `auto` switch or mark was refused, if it was. */
  autoError = $state<string | null>(null);

  pending = $derived(this.relay?.pending ?? null);
  stopped = $derived(this.relay?.stopped ?? false);
  inUse = $derived(relayInUse(this.relay));
  /** The floor changed again after the author approved what was shown. */
  changedAgain = $derived(
    this.refused !== null && this.pending !== null && this.pending.hash !== this.refused.hash,
  );

  async start(): Promise<() => void> {
    const unlisten = await ipc.onRelay((relay) => {
      this.relay = relay;
      if (relay.pending === null) this.refused = null;
    });
    const initial = await ipc.getRelay();
    if (initial && !this.relay) this.relay = initial;
    return unlisten;
  }

  /** Approves the displayed `hash`, never another one (AD-24). */
  async approve(hash: string): Promise<void> {
    if (this.approving) return;
    this.approving = true;
    this.refused = null;
    try {
      await ipc.approveCatalogue(hash);
    } catch (error) {
      this.refused = { hash, message: ipc.errorMessage(error) };
    } finally {
      this.approving = false;
    }
  }

  /** Acknowledges anomaly `key`, on a pointer click only, as a decision. */
  async acknowledge(key: string): Promise<void> {
    if (this.acknowledging[key]) return;
    this.acknowledging = { ...this.acknowledging, [key]: true };
    this.anomalyError = null;
    try {
      await ipc.acknowledgeAnomaly(key);
    } catch (error) {
      this.anomalyError = ipc.errorMessage(error);
    } finally {
      this.acknowledging = { ...this.acknowledging, [key]: false };
    }
  }

  /** Turns auto approval on or off (AD-88). On is sent on a pointer click
   *  only, by the caller (D-28); the backend refuses it without the gate. */
  async setAutoApproval(on: boolean): Promise<void> {
    await this.autoCommand(() => ipc.setAutoApproval(on));
  }

  /** Sets auto approval's mode (D-162). */
  async setAutoMode(mode: AutoMode): Promise<void> {
    await this.autoCommand(() => ipc.setAutoMode(mode));
  }

  /** Marks run `runId` "should not have run" (AD-88, FR-9), on a pointer
   *  click only, by the caller. */
  async markShouldNotHaveRun(runId: string): Promise<void> {
    await this.autoCommand(() => ipc.markShouldNotHaveRun(runId));
  }

  /** One `auto` command at a time; its refusal kept to show. The next
   *  relay view shows what changed. */
  private async autoCommand(command: () => Promise<void>): Promise<void> {
    if (this.autoBusy) return;
    this.autoBusy = true;
    this.autoError = null;
    try {
      await command();
    } catch (error) {
      this.autoError = ipc.errorMessage(error);
    } finally {
      this.autoBusy = false;
    }
  }

  async setStopped(stopped: boolean): Promise<void> {
    if (this.switching) return;
    this.switching = true;
    this.stopError = null;
    try {
      await ipc.setRelayStopped(stopped);
    } catch (error) {
      this.stopError = ipc.errorMessage(error);
    } finally {
      this.switching = false;
    }
  }
}
