// The Orchestrator view in the window (AD-92, AD-93): the last view the
// collector emitted, the author's reading of its briefing, and the card in
// flight beside it (story 5.9): what the composer chose, the card core
// issued for it, and its launch. The card is core's (AD-78): the window
// shows it and sends its id and prompt hash back, never a prompt, a hash or
// an argument of its own.
import type { CardAnswer } from "./generated/CardAnswer";
import type { CardChange } from "./generated/CardChange";
import type { IssueRefusal } from "./generated/IssueRefusal";
import type { LaunchAnswer } from "./generated/LaunchAnswer";
import type { LaunchRefusal } from "./generated/LaunchRefusal";
import type { OrchestratorView } from "./generated/OrchestratorView";
import type { StartAnswer } from "./generated/StartAnswer";
import type { SubjectChoice } from "./generated/SubjectChoice";
import type { TemplateOption } from "./generated/TemplateOption";
import type { WorkOrderCard } from "./generated/WorkOrderCard";
import * as ipc from "./ipc";

/** What the composer holds: a template, a project and each parameter's
 *  value, every one picked from its list; "" while not chosen. */
export interface Choices {
  template: string;
  project: string;
  values: Record<string, string>;
}

/** Why the composer shows no new card: core's typed refusal of an issue,
 *  an edit or a launch, or a failure of the machinery as text. */
export type ComposerFault =
  | { kind: "issue"; refusal: IssueRefusal }
  | { kind: "launch"; refusal: LaunchRefusal }
  | { kind: "error"; message: string };

const NOTHING_CHOSEN: Choices = { template: "", project: "", values: {} };

/** The choices a template starts with: only what has a single option is
 *  chosen for the author (D-680). */
function initialChoices(template: TemplateOption): Choices {
  const only = (list: string[]): string => (list.length === 1 ? list[0]! : "");
  return {
    template: template.name,
    project: only(template.projects),
    values: Object.fromEntries(Object.entries(template.params).map(([name, list]) => [name, only(list)])),
  };
}

/** The choices a card stands for. */
function choicesOf(card: WorkOrderCard): Choices {
  return {
    template: card.template,
    project: card.project,
    values: Object.fromEntries(card.values.map((param) => [param.name, param.value])),
  };
}

export class OrchestratorStore {
  view = $state<OrchestratorView | null>(null);
  /** Why the last reading could not be recorded, if it could not. */
  readError = $state<string | null>(null);

  choices = $state<Choices>({ ...NOTHING_CHOSEN });
  /** The card on screen: the last one core issued for the choices. */
  card = $state<WorkOrderCard | null>(null);
  /** Why the last issue, edit or launch gave no card or no launch. */
  fault = $state<ComposerFault | null>(null);
  /** An issue or an edit is on its way. */
  composing = $state(false);
  /** The card's launch is on its way. */
  launching = $state(false);
  /** How the card's launch was answered, once it was: the card is then
   *  done, and a new choice issues a new card. */
  launch = $state<LaunchAnswer | null>(null);
  /** A Start is on its way (AD-89). */
  starting = $state(false);
  /** How the last Start was answered, or why it could not be sent. */
  started = $state<StartAnswer | null>(null);
  startError = $state<string | null>(null);
  /** Why the last dismissal of a proposal failed, and which. */
  proposalError = $state<{ id: string; message: string } | null>(null);
  /** Numbers the issue and edit requests: only the latest one's answer is
   *  shown, so a slow answer never replaces a newer card. */
  private request = 0;

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

  /** Starts the orchestrator, or brings the live one forward (AD-89);
   *  one Start at a time. */
  async startOrchestrator(): Promise<void> {
    if (this.starting) return;
    this.starting = true;
    this.startError = null;
    try {
      this.started = await ipc.startOrchestrator();
    } catch (error) {
      this.started = null;
      this.startError = ipc.errorMessage(error);
    } finally {
      this.starting = false;
    }
  }

  /** The approved template the composer is on, if the view still offers it. */
  get template(): TemplateOption | null {
    return this.view?.templates.find((t) => t.name === this.choices.template) ?? null;
  }

  /** A card that is open and not launched: a choice edits it. */
  private get editable(): WorkOrderCard | null {
    return this.card && this.card.state === "open" && this.launch === null && !this.launching ? this.card : null;
  }

  async chooseTemplate(name: string): Promise<void> {
    const template = this.view?.templates.find((t) => t.name === name);
    this.choices = template ? initialChoices(template) : { ...NOTHING_CHOSEN };
    this.drop();
    await this.compose();
  }

  async chooseProject(project: string): Promise<void> {
    this.choices.project = project;
    this.drop();
    await this.compose();
  }

  /** A parameter's value: an open card is edited (AD-78), else a card is
   *  issued once every choice is made. */
  async chooseValue(name: string, value: string): Promise<void> {
    this.choices.values[name] = value;
    const card = this.editable;
    if (card) await this.edit(card, { kind: "value", name, value });
    else await this.compose();
  }

  /** The subject the card files its session under. */
  async chooseSubject(subject: SubjectChoice): Promise<void> {
    const card = this.editable;
    if (card) await this.edit(card, { kind: "subject", subject });
    else await this.compose(subject);
  }

  /** Opens the pending work order proposal `id` as the card on screen
   *  (AD-91, D-152): core issues it from the stored values, through the
   *  same validation as the composer's; the composer then shows its
   *  choices, and a choice edits it. */
  async openProposal(id: string): Promise<void> {
    this.drop();
    await this.ask(() => ipc.issueCardFromProposal(id), null);
  }

  /** Dismisses proposal `id` (FR-12); the next view shows it decided. */
  async dismissProposal(id: string): Promise<void> {
    this.proposalError = null;
    try {
      await ipc.dismissProposal(id);
    } catch (error) {
      this.proposalError = { id, message: ipc.errorMessage(error) };
    }
  }

  /** Launches the card on screen: the caller sends it on a pointer click
   *  only (D-28). */
  async launchCard(): Promise<void> {
    const card = this.editable;
    if (!card) return;
    this.launching = true;
    this.fault = null;
    try {
      const answer = await ipc.launchWorkOrder(card.id, card.prompt_sha256);
      if (this.card?.id !== card.id) return;
      if (answer.outcome === "refused" && answer.value === "unknown_card") {
        this.card = null;
        this.fault = { kind: "launch", refusal: answer.value };
      } else {
        this.launch = answer;
      }
    } catch (error) {
      this.fault = { kind: "error", message: ipc.errorMessage(error) };
    } finally {
      this.launching = false;
    }
  }

  /** Clears the card and its launch: the choices no longer match them. */
  private drop(): void {
    this.request += 1;
    this.composing = false;
    this.card = null;
    this.launch = null;
    this.fault = null;
  }

  /** Issues a card for the choices once every one is made. */
  private async compose(subject: SubjectChoice | null = null): Promise<void> {
    const { template, project, values } = this.choices;
    const entries = Object.entries(values);
    if (template === "" || project === "" || entries.some(([, value]) => value === "")) {
      this.drop();
      return;
    }
    await this.ask(() => ipc.issueCard({ template, project, values: entries, subject }), null);
  }

  private async edit(card: WorkOrderCard, change: CardChange): Promise<void> {
    await this.ask(() => ipc.editCard(card.id, change), card);
  }

  /** Sends one issue or edit and shows its answer, if no later one was
   *  sent meanwhile. A refused edit keeps its card and puts the choices
   *  back to it; a card the backend no longer holds open is dropped. */
  private async ask(send: () => Promise<CardAnswer>, edited: WorkOrderCard | null): Promise<void> {
    const request = ++this.request;
    this.composing = true;
    this.fault = null;
    try {
      const answer = await send();
      if (request !== this.request) return;
      if (answer.outcome === "issued") {
        this.card = answer.value;
        this.launch = null;
        this.choices = choicesOf(answer.value);
        return;
      }
      this.fault = { kind: "issue", refusal: answer.value };
      const gone = answer.value.kind === "unknown_card" || answer.value.kind === "card_not_open";
      if (edited && !gone) this.choices = choicesOf(edited);
      else this.card = null;
    } catch (error) {
      if (request !== this.request) return;
      this.fault = { kind: "error", message: ipc.errorMessage(error) };
      if (edited) this.choices = choicesOf(edited);
    } finally {
      if (request === this.request) this.composing = false;
    }
  }
}
