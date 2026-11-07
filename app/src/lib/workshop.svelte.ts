// The workshop's state in the window (AD-121, AD-122, D-249): the last
// Workshop view, the review panel's run, view, approvals in flight and
// answers, and the Workshop section's actions and their answers. Every
// call goes through the `ipc.ts` wrappers. The panel takes the meeting's
// slot, one panel at a time (AD-84).
import * as ipc from "./ipc";
import type { ApprovalRefusal } from "./generated/ApprovalRefusal";
import type { BatteryAnswer } from "./generated/BatteryAnswer";
import type { BuildAnswer } from "./generated/BuildAnswer";
import type { CheckLoginAnswer } from "./generated/CheckLoginAnswer";
import type { DiscardAnswer } from "./generated/DiscardAnswer";
import type { DiscardReason } from "./generated/DiscardReason";
import type { ImportAnswer } from "./generated/ImportAnswer";
import type { PrepareLoginAnswer } from "./generated/PrepareLoginAnswer";
import type { RemoveRunAnswer } from "./generated/RemoveRunAnswer";
import type { ResumeAnswer } from "./generated/ResumeAnswer";
import type { ResumeChoice } from "./generated/ResumeChoice";
import type { ReviewView } from "./generated/ReviewView";
import type { StopAnswer } from "./generated/StopAnswer";
import type { WorkshopView } from "./generated/WorkshopView";
import { meeting } from "./meeting.svelte";

/** The run id of a workshop board card's key, `ws:<run id>` (AD-121). */
export function runIdOf(key: string): string | null {
  return key.startsWith("ws:") ? key.slice(3) : null;
}

export class WorkshopStore {
  view = $state<WorkshopView | null>(null);
  /** The run the review panel shows, `null` while it is closed. */
  runId = $state<string | null>(null);
  review = $state<ReviewView | null>(null);
  error = $state<string | null>(null);
  /** Approvals on their way, by group key. */
  approving = $state<Record<string, boolean>>({});
  approvalErrors = $state<Record<string, ApprovalRefusal | string>>({});
  importing = $state(false);
  imported = $state<ImportAnswer | null>(null);
  discarding = $state(false);
  discarded = $state<DiscardAnswer | null>(null);
  battery = $state<BatteryAnswer | null>(null);
  batteryError = $state<string | null>(null);
  /** The section's last answer of each action, and its failure. */
  building = $state(false);
  build = $state<BuildAnswer | null>(null);
  buildError = $state<string | null>(null);
  stopping = $state(false);
  stopped = $state<StopAnswer | null>(null);
  stopError = $state<string | null>(null);
  resuming = $state(false);
  resumed = $state<ResumeAnswer | null>(null);
  resumeError = $state<string | null>(null);
  /** Removals on their way, and their answers, by run id. */
  removing = $state<Record<string, boolean>>({});
  removed = $state<Record<string, RemoveRunAnswer | string>>({});
  login = $state<PrepareLoginAnswer | null>(null);
  loginError = $state<string | null>(null);
  checkingLogin = $state(false);
  loginCheck = $state<CheckLoginAnswer | null>(null);
  #asked = 0;
  #returnFocus: HTMLElement | null = null;

  async start(): Promise<() => void> {
    const unlisten = await ipc.onWorkshop((view) => (this.view = view));
    const first = await ipc.getWorkshop();
    if (first && this.view === null) this.view = first;
    return unlisten;
  }

  /** Receive on a workshop card: the meeting closes, the panel opens on
   *  run `runId` and reads its review. */
  async open(runId: string): Promise<void> {
    meeting.close(false);
    if (this.runId === null) {
      const active = document.activeElement;
      this.#returnFocus = active instanceof HTMLElement ? active : null;
    }
    this.runId = runId;
    this.review = null;
    this.error = null;
    this.approving = {};
    this.approvalErrors = {};
    this.imported = null;
    this.discarded = null;
    await this.#read(runId);
  }

  /** Closes the panel and gives the focus back to where Receive was. */
  close(): void {
    if (this.runId === null) return;
    this.#asked += 1;
    this.runId = null;
    this.review = null;
    this.error = null;
    const back = this.#returnFocus;
    this.#returnFocus = null;
    if (back?.isConnected) back.focus();
  }

  async #read(runId: string): Promise<void> {
    const asked = ++this.#asked;
    try {
      const review = await ipc.openReview(runId);
      if (asked === this.#asked) this.review = review;
    } catch (error) {
      if (asked === this.#asked) this.error = ipc.errorMessage(error);
    }
  }

  /** Approve on group `key`, with the hash the panel showed: the caller
   *  sends it on a pointer click only (D-28). */
  async approve(key: string): Promise<void> {
    const review = this.review;
    const group = review?.groups.find((g) => g.key === key);
    if (!review || !group?.hunks_sha256 || this.approving[key]) return;
    this.approving[key] = true;
    delete this.approvalErrors[key];
    try {
      const answer = await ipc.approveSensitive(review.run_id, key, group.hunks_sha256);
      if (answer.outcome === "refused") this.approvalErrors[key] = answer.detail;
      if (this.runId === review.run_id) await this.#read(review.run_id);
    } catch (error) {
      this.approvalErrors[key] = ipc.errorMessage(error);
    } finally {
      delete this.approving[key];
    }
  }

  /** Import with the tip the panel showed: on a pointer click only. */
  async importPackage(): Promise<void> {
    const review = this.review;
    if (!review?.tip || this.importing) return;
    this.importing = true;
    try {
      this.imported = await ipc.importPackage(review.run_id, review.tip);
      if (this.runId === review.run_id) await this.#read(review.run_id);
    } catch (error) {
      this.imported = null;
      this.error = ipc.errorMessage(error);
    } finally {
      this.importing = false;
    }
  }

  /** Discard with one of the author's reasons: it only reduces, so the
   *  keyboard reaches it (AD-122). */
  async discard(reason: DiscardReason): Promise<void> {
    const review = this.review;
    if (!review || this.discarding) return;
    this.discarding = true;
    try {
      this.discarded = await ipc.discardPackage(review.run_id, reason);
      if (this.runId === review.run_id) await this.#read(review.run_id);
    } catch (error) {
      this.discarded = null;
      this.error = ipc.errorMessage(error);
    } finally {
      this.discarding = false;
    }
  }

  /** Run escape battery from a card whose image's battery is stale: on a
   *  pointer click only (D-28). */
  async runBattery(image: string | null): Promise<void> {
    this.batteryError = null;
    try {
      this.battery = await ipc.runBattery(image);
    } catch (error) {
      this.battery = null;
      this.batteryError = ipc.errorMessage(error);
    }
  }

  /** Build images (AD-99): on a pointer click only, each build starting
   *  containers. Each image's outcome reaches the view as it ends. */
  async buildImages(): Promise<void> {
    if (this.building) return;
    this.building = true;
    this.buildError = null;
    try {
      this.build = await ipc.buildImages();
    } catch (error) {
      this.build = null;
      this.buildError = ipc.errorMessage(error);
    } finally {
      this.building = false;
    }
  }

  /** Stop all (AD-116): it only reduces, so the keyboard reaches it. */
  async stopAll(): Promise<void> {
    if (this.stopping) return;
    this.stopping = true;
    this.stopError = null;
    try {
      this.stopped = await ipc.stopWorkshop();
    } catch (error) {
      this.stopped = null;
      this.stopError = ipc.errorMessage(error);
    } finally {
      this.stopping = false;
    }
  }

  /** Resume (AD-116) with each kept run's choice: on a pointer click only. */
  async resume(choices: ResumeChoice[], resumeRelay: boolean): Promise<boolean> {
    if (this.resuming) return false;
    this.resuming = true;
    this.resumeError = null;
    try {
      this.resumed = await ipc.resumeWorkshop(choices, resumeRelay);
      return true;
    } catch (error) {
      this.resumed = null;
      this.resumeError = ipc.errorMessage(error);
      return false;
    } finally {
      this.resuming = false;
    }
  }

  /** Removes a kept or orphan run's objects: it only reduces. */
  async removeRun(runId: string): Promise<void> {
    if (this.removing[runId]) return;
    this.removing[runId] = true;
    try {
      this.removed[runId] = await ipc.removeRun(runId);
    } catch (error) {
      this.removed[runId] = ipc.errorMessage(error);
    } finally {
      delete this.removing[runId];
    }
  }

  /** Log in for the workshop (D-605): on a pointer click only, since it
   *  creates the login volume. Answers the command the author runs. */
  async prepareLogin(): Promise<void> {
    this.loginError = null;
    try {
      this.login = await ipc.prepareLogin();
    } catch (error) {
      this.login = null;
      this.loginError = ipc.errorMessage(error);
    }
  }

  /** Check login (D-608): on a pointer click only, since it starts the
   *  probe's container. */
  async checkLogin(): Promise<void> {
    if (this.checkingLogin) return;
    this.checkingLogin = true;
    this.loginError = null;
    try {
      this.loginCheck = await ipc.checkLogin();
    } catch (error) {
      this.loginCheck = null;
      this.loginError = ipc.errorMessage(error);
    } finally {
      this.checkingLogin = false;
    }
  }
}

export const workshop = new WorkshopStore();
