// The registry in the window (AD-55): the last registry view, the archived
// list when asked for, the sessions the author selected, and the author's
// requests in flight with why the last one of each target was refused.
// Nothing changes on screen until a new view arrives from the collector.
import type { RegistryView } from "./generated/RegistryView";
import type { SessionView } from "./generated/SessionView";
import type { SubjectOrigin } from "./generated/SubjectOrigin";
import type { SubjectView } from "./generated/SubjectView";
import * as ipc from "./ipc";
import { AUTHOR_REFUSAL_TEXT } from "./registry-text";

/** What a request acts on: one subject, the selected sessions, a new
 *  subject in a chosen project, or the archived list. Requests and their
 *  refusals are kept per target. */
export type Target = `subject:${number}` | "selection" | "create" | "archived";

export function subjectTarget(id: number): Target {
  return `subject:${id}`;
}

/** What a rejected author request says: the wording of the rule that
 *  refused it, or the error's own message. */
export function refusalText(error: unknown): string {
  if (ipc.isIpcError(error) && error.refusal) return AUTHOR_REFUSAL_TEXT[error.refusal];
  return ipc.errorMessage(error);
}

/** Every session the view lists: those of its subjects and the unfiled
 *  ones, non-interactive runs included. */
export function allSessions(view: RegistryView | null): SessionView[] {
  if (!view) return [];
  return [
    ...view.subjects.flatMap((s) => s.sessions),
    ...view.unfiled.flatMap((p) => [...p.sessions, ...p.non_interactive]),
  ];
}

/** True for a session that runs now: not ended, and on the board. */
export function isLive(session: SessionView): boolean {
  return session.ended === null && session.card !== null;
}

export class RegistryStore {
  view = $state<RegistryView | null>(null);
  /** The archived subjects, once asked for; `null` until then. */
  archived = $state<SubjectView[] | null>(null);
  /** The sessions selected for a move, unfile or new subject. */
  selected = $state<string[]>([]);
  inFlight = $state<Record<string, boolean>>({});
  errors = $state<Record<string, string>>({});

  /** The open subjects, those a session may be filed under. */
  openSubjects = $derived(this.view?.subjects ?? []);
  /** The one project of the selected sessions, or `null` when they span
   *  several or none is selected. */
  selectionProject = $derived.by(() => {
    const projects = allSessions(this.view)
      .filter((s) => this.selected.includes(s.session_id))
      .map((s) => s.project);
    const first = projects[0];
    return first !== undefined && projects.every((p) => p === first) ? first : null;
  });

  async start(): Promise<() => void> {
    const unlisten = await ipc.onRegistry((view) => {
      this.view = view;
      const known = allSessions(view).map((s) => s.session_id);
      this.selected = this.selected.filter((id) => known.includes(id));
    });
    const initial = await ipc.getRegistry();
    if (initial && !this.view) this.view = initial;
    return unlisten;
  }

  toggle(sessionId: string): void {
    this.selected = this.selected.includes(sessionId)
      ? this.selected.filter((id) => id !== sessionId)
      : [...this.selected, sessionId];
  }

  /** Runs `call` once at a time per target; a rejection is kept to show
   *  beside the target, and the screen otherwise stays as it is. True when
   *  the backend accepted it. */
  async #run(target: Target, call: () => Promise<unknown>): Promise<boolean> {
    if (this.inFlight[target]) return false;
    this.inFlight = { ...this.inFlight, [target]: true };
    const { [target]: _previous, ...others } = this.errors;
    this.errors = others;
    try {
      await call();
      return true;
    } catch (error) {
      this.errors = { ...this.errors, [target]: refusalText(error) };
      return false;
    } finally {
      const { [target]: _done, ...rest } = this.inFlight;
      this.inFlight = rest;
    }
  }

  /** A new subject in `project`, one the registry knows (D-95). */
  create(name: string, project: string): Promise<boolean> {
    return this.#run("create", () => ipc.createSubject(name, { project }));
  }

  /** A new subject from the selected sessions, which give it their project
   *  and are filed under it. */
  async createFromSelection(name: string): Promise<boolean> {
    const origin: SubjectOrigin = { sessions: [...this.selected] };
    const done = await this.#run("selection", () => ipc.createSubject(name, origin));
    if (done) this.selected = [];
    return done;
  }

  /** Files the selected sessions under `subject`, or unfiles them. */
  async fileSelection(subject: number | null): Promise<boolean> {
    const sessions = [...this.selected];
    const done = await this.#run("selection", () => ipc.fileSessions(sessions, subject));
    if (done) this.selected = [];
    return done;
  }

  rename(subject: number, name: string): Promise<boolean> {
    return this.#run(subjectTarget(subject), () => ipc.renameSubject(subject, name));
  }

  merge(source: number, target: number): Promise<boolean> {
    return this.#run(subjectTarget(source), () => ipc.mergeSubjects(source, target));
  }

  async archive(subject: number, routine: boolean): Promise<boolean> {
    const done = await this.#run(subjectTarget(subject), () => ipc.archiveSubject(subject, routine));
    if (done && this.archived !== null) await this.loadArchived();
    return done;
  }

  async reopen(subject: number): Promise<boolean> {
    const done = await this.#run(subjectTarget(subject), () => ipc.reopenSubject(subject));
    if (done) await this.loadArchived();
    return done;
  }

  /** Asks the collector for the archived subjects. */
  async loadArchived(): Promise<void> {
    await this.#run("archived", async () => {
      this.archived = await ipc.listArchived();
    });
  }
}
