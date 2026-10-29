// The Orchestrator view in the window (AD-92, AD-93): the last view the
// collector emitted, and the author's reading of its briefing. The cards in
// flight join it with the composer (story 5.9).
import type { OrchestratorView } from "./generated/OrchestratorView";
import * as ipc from "./ipc";

export class OrchestratorStore {
  view = $state<OrchestratorView | null>(null);
  /** Why the last reading could not be recorded, if it could not. */
  readError = $state<string | null>(null);

  async start(): Promise<() => void> {
    const unlisten = await ipc.onOrchestrator((view) => (this.view = view));
    const initial = await ipc.getOrchestrator();
    if (initial && !this.view) this.view = initial;
    return unlisten;
  }

  /** The author leaves the view: its briefing is read (AD-83), and the
   *  next one counts its changes from now. */
  async markRead(): Promise<void> {
    this.readError = null;
    try {
      await ipc.markBriefingRead();
    } catch (error) {
      this.readError = ipc.errorMessage(error);
    }
  }
}
