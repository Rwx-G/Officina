// The Orchestrator store's card in flight (AD-78, AD-93): an edit replaces
// the card with the one core issued, a card the backend no longer knows
// leaves the screen with its refusal, and a refused edit puts the choices
// back to the card still shown. Cards are core's, from the e2e fixtures.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fixtureOf } from "../../e2e/fixture-index";
import type { CardAnswer } from "./generated/CardAnswer";
import type { OrchestratorView } from "./generated/OrchestratorView";
import type { WorkOrderCard } from "./generated/WorkOrderCard";
import * as ipc from "./ipc";
import { OrchestratorStore } from "./orchestrator.svelte";

vi.mock("./ipc", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./ipc")>()),
  issueCard: vi.fn(),
  editCard: vi.fn(),
  launchWorkOrder: vi.fn(),
}));

const issueCard = vi.mocked(ipc.issueCard);
const editCard = vi.mocked(ipc.editCard);
const launchWorkOrder = vi.mocked(ipc.launchWorkOrder);

const card = (name: string): WorkOrderCard => fixtureOf<WorkOrderCard>("card", name);
const issued = (value: WorkOrderCard): CardAnswer => ({ outcome: "issued", value });

/** A store on the day's view, its first card composed. */
async function composed(): Promise<{ store: OrchestratorStore; first: WorkOrderCard }> {
  const store = new OrchestratorStore();
  store.view = fixtureOf<OrchestratorView>("orchestrator", "orchestrator-day");
  const first = card("card-bump");
  issueCard.mockResolvedValueOnce(issued(first));
  await store.chooseTemplate(first.template);
  await store.chooseProject(first.project);
  for (const param of first.values) await store.chooseValue(param.name, param.value);
  expect(store.card).toEqual(first);
  return { store, first };
}

describe("OrchestratorStore cards", () => {
  beforeEach(() => {
    issueCard.mockReset();
    editCard.mockReset();
    launchWorkOrder.mockReset();
  });

  it("issues one card once every choice is made", async () => {
    const { first } = await composed();
    expect(issueCard).toHaveBeenCalledTimes(1);
    const request = issueCard.mock.calls[0]![0];
    expect(request.template).toBe(first.template);
    expect(request.project).toBe(first.project);
    expect(Object.fromEntries(request.values)).toEqual(Object.fromEntries(first.values.map((p) => [p.name, p.value])));
    expect(request.subject).toBeNull();
  });

  it("a superseded card is replaced by the new one", async () => {
    const { store, first } = await composed();
    const edited = card("card-bump-minor");
    const changed = edited.values.find((p) => p.value !== first.values.find((q) => q.name === p.name)!.value)!;
    editCard.mockResolvedValueOnce(issued(edited));
    await store.chooseValue(changed.name, changed.value);
    expect(editCard).toHaveBeenCalledWith(first.id, { kind: "value", name: changed.name, value: changed.value });
    expect(store.card).toEqual(edited);
    expect(store.choices.values[changed.name]).toBe(changed.value);
    expect(store.fault).toBeNull();
  });

  it("an unknown card is dropped with its refusal", async () => {
    const { store, first } = await composed();
    const other = first.values[0]!.options.find((o) => o !== first.values[0]!.value)!;
    editCard.mockResolvedValueOnce({ outcome: "refused", value: { kind: "unknown_card" } });
    await store.chooseValue(first.values[0]!.name, other);
    expect(store.card).toBeNull();
    expect(store.fault).toEqual({ kind: "issue", refusal: { kind: "unknown_card" } });

    // The same at a launch.
    const again = await composed();
    launchWorkOrder.mockResolvedValueOnce({ outcome: "refused", value: "unknown_card" });
    await again.store.launchCard();
    expect(again.store.card).toBeNull();
    expect(again.store.fault).toEqual({ kind: "launch", refusal: "unknown_card" });
  });

  it("a refused edit keeps its card and puts the choices back", async () => {
    const { store, first } = await composed();
    const param = first.values[0]!;
    const other = param.options.find((o) => o !== param.value)!;
    editCard.mockResolvedValueOnce({ outcome: "refused", value: { kind: "order", detail: "value_not_in_list" } });
    await store.chooseValue(param.name, other);
    expect(store.card).toEqual(first);
    expect(store.choices.values[param.name]).toBe(param.value);
    expect(store.fault).toEqual({ kind: "issue", refusal: { kind: "order", detail: "value_not_in_list" } });
  });

  it("a launch sends the card's id and prompt hash once, and a choice after it issues a new card", async () => {
    const { store, first } = await composed();
    launchWorkOrder.mockResolvedValueOnce({ outcome: "launched", value: "started" });
    await store.launchCard();
    await store.launchCard();
    expect(launchWorkOrder).toHaveBeenCalledTimes(1);
    expect(launchWorkOrder).toHaveBeenCalledWith(first.id, first.prompt_sha256, null);
    expect(store.launch).toEqual({ outcome: "launched", value: "started" });
    // The launched card is not edited: a new one is issued.
    const param = first.values[0]!;
    issueCard.mockResolvedValueOnce(issued(card("card-bump-minor")));
    await store.chooseValue(param.name, param.options.find((o) => o !== param.value)!);
    expect(editCard).not.toHaveBeenCalled();
    expect(issueCard).toHaveBeenCalledTimes(2);
    expect(store.launch).toBeNull();
  });

  it("a workshop card's launch sends the plan hash it shows (AD-107)", async () => {
    const { store, first } = await composed();
    const plan = "a".repeat(64);
    const workshop = { ...first, workshop: { plan_sha256: plan } } as unknown as WorkOrderCard;
    store.card = workshop;
    launchWorkOrder.mockResolvedValueOnce({ outcome: "launched", value: "started" });
    await store.launchCard();
    expect(launchWorkOrder).toHaveBeenCalledWith(first.id, first.prompt_sha256, plan);
  });

  it("a slow answer never replaces a newer card", async () => {
    const { store, first } = await composed();
    let answerLate: (answer: CardAnswer) => void = () => {};
    editCard.mockImplementationOnce(() => new Promise((resolve) => (answerLate = resolve)));
    const param = first.values[0]!;
    const late = store.chooseValue(param.name, param.options.find((o) => o !== param.value)!);
    // The author picks another template meanwhile: the late edit is dropped.
    await store.chooseTemplate(first.template);
    answerLate(issued(card("card-bump-minor")));
    await late;
    expect(store.card).toBeNull();
    expect(store.composing).toBe(false);
  });
});
