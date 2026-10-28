// The scene's memory (AD-62 as amended, D-184): one storage read, pruned
// against the first board; writes only on change; storage may fail.
import { describe, expect, it } from "vitest";
import type { Board } from "../generated/Board";
import { layoutScene } from "./layout";
import { STORAGE_KEY, emptyMemory, parse, sceneMemoryStore, serialise, type StorageGetter } from "./memory";
import { board, card } from "./testing";

class FakeStorage {
  items = new Map<string, string>();
  reads = 0;
  writes: string[] = [];
  getItem(key: string): string | null {
    this.reads++;
    return this.items.get(key) ?? null;
  }
  setItem(key: string, value: string): void {
    this.writes.push(key);
    this.items.set(key, value);
  }
}

const stored = (floor: string | null, offices: string[], desks: Record<string, (string | null)[]>) =>
  JSON.stringify({ v: 2, floor, offices, desks });

function storeWith(raw: Record<string, string>) {
  const storage = new FakeStorage();
  for (const [k, v] of Object.entries(raw)) storage.items.set(k, v);
  return { storage, store: sceneMemoryStore(() => storage) };
}

const lay = (b: Board, memory = emptyMemory(b.floor)) => layoutScene({ board: b }, memory);

describe("memory", () => {
  it("load prunes offices and desks without a live card on the first board", () => {
    const { storage, store } = storeWith({
      [STORAGE_KEY]: stored("main", ["gone", "beta", "alpha"], { gone: ["g", null], beta: ["b-ended", "b"], alpha: ["a", null, null, null] }),
    });
    const first = board([card("b", "office", { project: "beta" }), card("a", "office", { project: "alpha" })], { floor: "main" });
    const memory = store.load(first);
    expect(memory).toMatchObject({ floor: "main", offices: ["beta", "alpha"], desks: { beta: [null, "b"], alpha: ["a", null, null, null] }, inRun: false });
    // Read once: a second load returns the same memory.
    expect(store.load(board([], { floor: "main" }))).toBe(memory);
    expect(storage.reads).toBe(1);
    // The remembered order and desks hold on the first layout.
    const l = lay(first, memory);
    expect(l.rooms.filter((r) => r.kind === "office").map((r) => r.project!.name)).toEqual(["beta", "alpha"]);
    expect(l.desks.find((d) => d.key === "b")).toMatchObject({ slot: 0, index: 1 });
  });

  it("load discards a memory of another floor", () => {
    const { store } = storeWith({ [STORAGE_KEY]: stored("home", ["beta"], { beta: ["b"] }) });
    const memory = store.load(board([card("b", "office", { project: "beta" })], { floor: "work" }));
    expect(memory).toEqual(emptyMemory("work"));
    const none = storeWith({ [STORAGE_KEY]: stored(null, ["beta"], {}) }).store.load(board([card("b", "office", { project: "beta" })], { floor: "work" }));
    expect(none.offices).toEqual([]);
  });

  it("the v1 key is ignored", () => {
    const { storage, store } = storeWith({ "officina.scene.v1": JSON.stringify({ v: 1, slots: ["beta"], desks: { beta: ["b"] } }) });
    const memory = store.load(board([card("b", "office", { project: "beta" })]));
    expect(memory).toEqual(emptyMemory(null));
    store.commit(lay(board([card("b", "office", { project: "beta" })]), memory).memory);
    expect(storage.writes).toEqual([STORAGE_KEY]);
    expect(storage.items.get("officina.scene.v1")).toContain('"v":1');
  });

  it("commit writes only when the persisted assignment changes", () => {
    const { storage, store } = storeWith({});
    const b = board([card("a", "office")]);
    const first = lay(b, store.load(b));
    store.commit(first.memory);
    expect(storage.writes).toHaveLength(1);
    expect(parse(storage.items.get(STORAGE_KEY)!)).toEqual({ floor: null, offices: ["alpha"], desks: { alpha: ["a", null, null, null] } });
    // Same assignment, other in-memory state: no write.
    const second = lay(board([card("a", "lounge")]), store.current());
    expect(second.memory.sticky).not.toEqual(first.memory.sticky);
    store.commit(second.memory);
    expect(storage.writes).toHaveLength(1);
    expect(store.current()).toBe(second.memory);
    // A new desk: one write.
    store.commit(lay(board([card("a", "lounge"), card("c", "office", { started: 1 })]), store.current()).memory);
    expect(storage.writes).toHaveLength(2);
    // A stored value equal to the first commit is not written again.
    const again = storeWith({ [STORAGE_KEY]: serialise(first.memory) });
    again.store.commit(lay(b, again.store.load(b)).memory);
    expect(again.storage.writes).toEqual([]);
  });

  it("an unreadable or foreign value starts empty", () => {
    const b = board([card("a", "office")]);
    for (const raw of ["{", "null", "42", '"text"', "[]", '{"v":3,"floor":null,"offices":[],"desks":{}}', '{"v":2,"floor":7,"offices":[],"desks":{}}', '{"v":2,"floor":null,"offices":[1],"desks":{}}', '{"v":2,"floor":null,"offices":[],"desks":{"a":[1]}}', '{"v":2,"floor":null,"offices":[],"desks":[]}']) {
      expect(storeWith({ [STORAGE_KEY]: raw }).store.load(b), raw).toEqual(emptyMemory(null));
    }
  });

  it("a storage that throws keeps the layout working", () => {
    const failing = {
      getItem(): string | null {
        throw new Error("denied");
      },
      setItem(): void {
        throw new Error("quota");
      },
    };
    const getters: StorageGetter[] = [
      () => failing,
      () => {
        throw new Error("SecurityError");
      },
      () => null,
    ];
    for (const getter of getters) {
      const store = sceneMemoryStore(getter);
      const b = board([card("a", "office"), card("d", "decision", { started: 1 })]);
      const memory = store.load(b);
      expect(memory).toEqual(emptyMemory(null));
      const l = layoutScene({ board: b }, memory);
      expect(() => store.commit(l.memory)).not.toThrow();
      expect(store.current()).toBe(l.memory);
      expect(layoutScene({ board: b }, store.current())).toEqual(l);
    }
  });
});
