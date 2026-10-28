// The registry store's rules (AD-55, AD-58's Vitest layer): which verdict a
// row is confirmed with, what Confirm all leaves out and why, the item a
// row sends, and the choices dropped with a row the view no longer lists.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { RegistryStore } from "./registry.svelte";
import type { RegistryView } from "./generated/RegistryView";
import type { TriageRow } from "./generated/TriageRow";

let deliver: ((view: RegistryView) => void) | null = null;

vi.mock("./ipc", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./ipc")>()),
  getRegistry: vi.fn(async () => null),
  onRegistry: vi.fn(async (handler: (view: RegistryView) => void) => {
    deliver = handler;
    return () => {};
  }),
}));

function row(key: string, overrides: Partial<TriageRow> = {}): TriageRow {
  return {
    key,
    kind: "subject",
    project: "api",
    subject: { id: 1, name: "certificates" },
    session_id: "s1",
    session_name: null,
    proposal: { verdict: "to_resume", source: "rules", reason: "default_to_resume", call_id: null },
    verdict: "to_resume",
    excerpts: null,
    end: null,
    proposed_subject: null,
    choices: [],
    model_pending: false,
    model_reason: null,
    model_call: null,
    ...overrides,
  };
}

const modelFinished = (key: string): TriageRow =>
  row(key, {
    proposal: { verdict: "finished", source: "model", reason: "model", call_id: 4 },
    verdict: "finished",
  });

/** A view holding `rows` and nothing else the store reads. */
function view(rows: TriageRow[]): RegistryView {
  return { triage: rows, subjects: [], unfiled: [] } as unknown as RegistryView;
}

async function storeWith(rows: TriageRow[]): Promise<RegistryStore> {
  const store = new RegistryStore();
  await store.start();
  deliver!(view(rows));
  return store;
}

describe("RegistryStore", () => {
  beforeEach(() => {
    deliver = null;
  });

  it("confirms with the author's verdict, else the one proposed", async () => {
    const shown = row("subject:1");
    const store = await storeWith([shown]);
    expect(store.verdictOf(shown)).toBe("to_resume");
    store.setVerdict(shown.key, "finished");
    expect(store.verdictOf(shown)).toBe("finished");
  });

  it("leaves out each row that cannot be confirmed, with its cause", async () => {
    const unsure = row("subject:2", {
      proposal: { verdict: "unsure", source: "model", reason: "model", call_id: 3 },
      verdict: null,
    });
    const homeless = row("session:s9", { kind: "unfiled_session", subject: null, proposed_subject: null });
    const filed = row("session:s8", { kind: "unfiled_session", subject: null, proposed_subject: 7 });
    const plain = row("subject:3");
    const store = await storeWith([unsure, homeless, filed, plain]);
    expect(store.leftOut(unsure)).toBe("unsure");
    expect(store.item(unsure)).toBeNull();
    expect(store.leftOut(homeless)).toBe("no_subject");
    expect(store.item(homeless)).toBeNull();
    expect(store.leftOut(filed)).toBeNull();
    expect(store.item(filed)?.file_under).toEqual({ subject: { id: 7 } });
    expect(store.leftOut(plain)).toBeNull();
    store.confirming = { [plain.key]: true };
    expect(store.leftOut(plain)).toBe("in_flight");
    expect(store.item(plain)).toBeNull();
  });

  // Security audit L2: a model's `finished` never goes in a batch.
  it("leaves a model's finished out of Confirm all and still sends it alone", async () => {
    const proposed = modelFinished("subject:4");
    const store = await storeWith([proposed]);
    expect(store.leftOut(proposed)).toBe("model_finished");
    expect(store.item(proposed)).toEqual({
      key: proposed.key,
      shown: proposed.proposal,
      verdict: "finished",
      routine: false,
      file_under: null,
    });
    store.setVerdict(proposed.key, "finished");
    expect(store.leftOut(proposed)).toBeNull();
    const toResume = row("subject:5", {
      proposal: { verdict: "to_resume", source: "model", reason: "model", call_id: 5 },
    });
    expect(store.leftOut(toResume)).toBeNull();
  });

  it("drops a row's choices with the row", async () => {
    const kept = row("subject:6");
    const gone = row("subject:7");
    const store = await storeWith([kept, gone]);
    store.setVerdict(gone.key, "finished");
    store.setRoutine(gone.key, true);
    store.setVerdict(kept.key, "finished");
    deliver!(view([kept]));
    expect(store.verdicts).toEqual({ [kept.key]: "finished" });
    expect(store.routines).toEqual({});
  });
});
