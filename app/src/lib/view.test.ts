import { describe, expect, it } from "vitest";
import {
  BASES,
  DEFAULT_BASE,
  VIEW_STORAGE_KEY,
  ViewStore,
  defaultViewState,
  readViewState,
  serialiseViewState,
} from "./view.svelte";

/** An in-memory storage. */
function memoryStorage(initial: Record<string, string> = {}) {
  const items = new Map(Object.entries(initial));
  return {
    items,
    getItem: (key: string) => items.get(key) ?? null,
    setItem: (key: string, value: string) => void items.set(key, value),
  };
}

const OTHER = BASES.find((base) => base !== DEFAULT_BASE)!;

describe("view state", () => {
  it("stored values are read field by field with their defaults", () => {
    expect(readViewState(null)).toEqual(defaultViewState());
    for (const raw of ["", "not json", "null", "42", "[]", '"scene"']) {
      expect(readViewState(raw), raw).toEqual(defaultViewState());
    }
    // A valid field is kept whatever else the value holds: another version,
    // fields a later story adds, or an invalid neighbour.
    for (const base of BASES) {
      expect(readViewState(JSON.stringify({ v: 1, base })).base).toBe(base);
      expect(readViewState(JSON.stringify({ v: 7, base, focus: true, theme: "dark" })).base).toBe(base);
    }
    expect(readViewState(JSON.stringify({ v: 1, base: "focus" })).base).toBe(DEFAULT_BASE);
    expect(readViewState(JSON.stringify({ v: 1, base: 3 })).base).toBe(DEFAULT_BASE);
    expect(readViewState(JSON.stringify({ v: 1 })).base).toBe(DEFAULT_BASE);
    expect(readViewState(serialiseViewState({ v: 1, base: OTHER }))).toEqual({ v: 1, base: OTHER });
  });

  it("Board is the default mode and Sessions the view at start", () => {
    expect(DEFAULT_BASE).toBe("board");
    const store = new ViewStore(() => memoryStorage());
    expect(store.base).toBe("board");
    expect(store.view).toBe("sessions");
  });

  it("a mode change is stored and read back by the next window", () => {
    const storage = memoryStorage();
    const store = new ViewStore(() => storage);
    store.toggleBase();
    expect(store.base).toBe(OTHER);
    expect(new ViewStore(() => storage).base).toBe(OTHER);
    store.toggleBase();
    expect(new ViewStore(() => storage).base).toBe(DEFAULT_BASE);
  });

  it("an unchanged mode writes nothing", () => {
    const storage = memoryStorage();
    const store = new ViewStore(() => storage);
    store.setBase(store.base);
    expect(storage.items.has(VIEW_STORAGE_KEY)).toBe(false);
  });

  it("a storage that throws keeps the defaults", () => {
    const throwing = {
      getItem: () => {
        throw new Error("denied");
      },
      setItem: () => {
        throw new Error("denied");
      },
    };
    const getterThrows = new ViewStore(() => {
      throw new Error("no storage");
    });
    expect(getterThrows.base).toBe(DEFAULT_BASE);
    const store = new ViewStore(() => throwing);
    expect(store.base).toBe(DEFAULT_BASE);
    // The mode still changes for this run.
    store.toggleBase();
    expect(store.base).toBe(OTHER);
    expect(new ViewStore(() => null).base).toBe(DEFAULT_BASE);
  });
});
