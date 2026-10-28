// The registry in the window (AD-55): the last registry view, the archived
// list when asked for, the sessions the author selected, the author's
// requests in flight with why the last one of each target was refused, and
// for each triage row the author's choices and whether it is being
// confirmed, all dropped with the row. Nothing changes on screen until a
// new view arrives from the collector.
import type { ConfirmItem } from "./generated/ConfirmItem";
import type { FileUnder } from "./generated/FileUnder";
import type { RegistryView } from "./generated/RegistryView";
import type { SessionView } from "./generated/SessionView";
import type { SubjectOrigin } from "./generated/SubjectOrigin";
import type { SubjectView } from "./generated/SubjectView";
import type { TriageRow } from "./generated/TriageRow";
import type { Verdict } from "./generated/Verdict";
import * as ipc from "./ipc";
import { AUTHOR_REFUSAL_TEXT, CONFIRM_REFUSAL_TEXT, NOT_NEEDED_TEXT } from "./registry-text";

/** Why Confirm all leaves a row out (AD-55, D-59): no verdict yet, no
 *  subject to file it under, or already being confirmed. */
export type LeftOut = "unsure" | "no_subject" | "in_flight";

/** Keeps the entries of `record` whose key is in `keys`. */
function only<T>(record: Record<string, T>, keys: string[]): Record<string, T> {
  return Object.fromEntries(Object.entries(record).filter(([key]) => keys.includes(key)));
}

/** What a request acts on: one subject, the selected sessions, a new
 *  subject in a chosen project, the archived list, or the dry run.
 *  Requests and their refusals are kept per target. */
export type Target =
  | `subject:${number}`
  | "selection"
  | "create"
  | "archived"
  | "evaluate"
  | "auto-archive"
  | "triage-model";

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

  /** The verdict the author set on a row with `f`, `t` or its buttons. */
  verdicts = $state<Record<string, Verdict>>({});
  /** Where the author chose to file an unfiled row. */
  filings = $state<Record<string, FileUnder>>({});
  /** The routine box of a row confirmed `finished` (D-62). */
  routines = $state<Record<string, boolean>>({});
  /** Rows sent and not yet dropped by a view: Confirm all leaves them out,
   *  and `Enter` waits for the view that removes them. */
  confirming = $state<Record<string, boolean>>({});
  /** Why the registry refused a row, shown on it; the row stays. */
  rowErrors = $state<Record<string, string>>({});
  /** What became of a filed session whose verdict its subject decides. */
  triageNote = $state<string | null>(null);
  /** Set by the header's "N to sort" chip; the Triage panel takes it and
   *  puts the focus on its first row. */
  openPanel = $state(false);

  /** The rows that need a verdict, the latest first. */
  rows = $derived<TriageRow[]>(this.view?.triage ?? []);

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
      this.#dropGoneRows();
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

  /** Turns auto-archive on or off (FR-12): the next view shows it. */
  setAutoArchive(on: boolean): Promise<boolean> {
    return this.#run("auto-archive", () => ipc.setAutoArchive(on));
  }

  /** Turns the triage model on or off (FR-10 as amended): the next view
   *  shows it, a pause lifted included. */
  setTriageModel(on: boolean): Promise<boolean> {
    return this.#run("triage-model", () => ipc.setTriageModel(on));
  }

  /** Queues a dry run of the triage model (AD-52): its progress and its
   *  result arrive with the next views. */
  evaluate(): Promise<boolean> {
    return this.#run("evaluate", () => ipc.evaluateTriageModel());
  }

  /** Asks the collector for the archived subjects. */
  async loadArchived(): Promise<void> {
    await this.#run("archived", async () => {
      this.archived = await ipc.listArchived();
    });
  }

  /** Drops the choices, flight marks and refusals of rows the view no
   *  longer lists (AD-55). */
  #dropGoneRows(): void {
    const keys = this.rows.map((row) => row.key);
    this.verdicts = only(this.verdicts, keys);
    this.filings = only(this.filings, keys);
    this.routines = only(this.routines, keys);
    this.confirming = only(this.confirming, keys);
    this.rowErrors = only(this.rowErrors, keys);
  }

  /** The verdict confirming `row` applies: the author's, else the one
   *  proposed; none for an `unsure` proposal until `f` or `t` (D-59). */
  verdictOf(row: TriageRow): Verdict | null {
    return this.verdicts[row.key] ?? row.verdict;
  }

  /** Where an unfiled row is filed: the author's choice, else the subject
   *  proposed; `null` for any other row, or when nothing is proposed. */
  fileUnderOf(row: TriageRow): FileUnder | null {
    if (row.kind !== "unfiled_session") return null;
    const proposed = row.proposed_subject;
    return this.filings[row.key] ?? (proposed === null ? null : { subject: { id: proposed } });
  }

  /** Why Confirm all leaves `row` out, or `null` when it takes it. */
  leftOut(row: TriageRow): LeftOut | null {
    if (this.confirming[row.key]) return "in_flight";
    if (this.verdictOf(row) === null) return "unsure";
    if (row.kind === "unfiled_session" && this.fileUnderOf(row) === null) return "no_subject";
    return null;
  }

  setVerdict(key: string, verdict: Verdict): void {
    this.verdicts = { ...this.verdicts, [key]: verdict };
  }

  fileUnder(key: string, target: FileUnder): void {
    this.filings = { ...this.filings, [key]: target };
  }

  setRoutine(key: string, routine: boolean): void {
    this.routines = { ...this.routines, [key]: routine };
  }

  /** The item confirming `row` as shown, or `null` when it cannot be sent. */
  item(row: TriageRow): ConfirmItem | null {
    const verdict = this.verdictOf(row);
    if (this.leftOut(row) !== null || verdict === null) return null;
    return {
      key: row.key,
      shown: row.proposal,
      verdict,
      routine: verdict === "finished" && (this.routines[row.key] ?? false),
      file_under: this.fileUnderOf(row),
    };
  }

  /** Confirms `rows` in one batch (AD-49). A row answered refused shows why
   *  and stays; a confirmed or filed one stays in flight until the view
   *  that drops it arrives. */
  async confirm(rows: TriageRow[]): Promise<void> {
    const items = rows.map((row) => this.item(row)).filter((item) => item !== null);
    if (items.length === 0) return;
    const keys = items.map((item) => item.key);
    this.confirming = { ...this.confirming, ...Object.fromEntries(keys.map((key) => [key, true])) };
    this.rowErrors = Object.fromEntries(Object.entries(this.rowErrors).filter(([key]) => !keys.includes(key)));
    const release = (key: string): void => {
      const { [key]: _released, ...rest } = this.confirming;
      this.confirming = rest;
    };
    try {
      const answers = await ipc.confirmTriage(items);
      for (const { key, outcome } of answers) {
        if (outcome.kind === "refused") {
          this.rowErrors = { ...this.rowErrors, [key]: CONFIRM_REFUSAL_TEXT[outcome.refusal] };
          release(key);
        } else if (outcome.kind === "filing_refused") {
          this.rowErrors = { ...this.rowErrors, [key]: AUTHOR_REFUSAL_TEXT[outcome.refusal] };
          release(key);
        } else if (outcome.kind === "verdict_not_needed") {
          this.triageNote = `The session was filed, and its verdict left to its subject's own row: ${NOT_NEEDED_TEXT[outcome.cause]}.`;
        }
      }
    } catch (error) {
      const why = refusalText(error);
      for (const key of keys) {
        this.rowErrors = { ...this.rowErrors, [key]: why };
        release(key);
      }
    }
    // The view that drops a confirmed row can arrive before the answer.
    this.#dropGoneRows();
  }
}
