// The relay's state in the window (AD-35): the last relay view, and the
// outcome of the author's approve and stop clicks.
import type { FieldChange } from "./generated/FieldChange";
import type { RelayView } from "./generated/RelayView";
import * as ipc from "./ipc";

/** The window's views (story 2.9): local window state, never part of the
 *  Rust contract. */
export type WindowView = "board" | "relay";

/** An approval the backend refused, kept to explain the banner. */
export interface RefusedApproval {
  /** The hash the author approved, as it was displayed. */
  hash: string;
  message: string;
}

/** True for a click made with a pointer. Enter and Space on a focused button
 *  also fire `click`, with `detail` 0: no key approves (AD-35, D-28). */
export function isPointerClick(event: MouseEvent): boolean {
  return event.detail > 0;
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
      this.refused = { hash, message: String(error) };
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
      this.anomalyError = String(error);
    } finally {
      this.acknowledging = { ...this.acknowledging, [key]: false };
    }
  }

  async setStopped(stopped: boolean): Promise<void> {
    if (this.switching) return;
    this.switching = true;
    this.stopError = null;
    try {
      await ipc.setRelayStopped(stopped);
    } catch (error) {
      this.stopError = String(error);
    } finally {
      this.switching = false;
    }
  }
}
