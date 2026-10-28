import { describe, expect, it } from "vitest";
import {
  BASES,
  DEFAULT_BASE,
  DEFAULT_FOCUS,
  DEFAULT_WALKING,
  VIEW_STORAGE_KEY,
  ViewStore,
  defaultViewState,
  readViewState,
  serialiseViewState,
  storedViewState,
} from "./view.svelte";
import { DEFAULT_THEME, THEME_CHOICES, applyTheme, nextTheme, resolveTheme } from "./theme";

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
    const other = { ...defaultViewState(), base: OTHER, walking: !DEFAULT_WALKING };
    expect(readViewState(serialiseViewState(other))).toEqual(other);
  });

  it("a stored view without theme reads as Auto", () => {
    expect(DEFAULT_THEME).toBe("auto");
    expect(defaultViewState().theme).toBe(DEFAULT_THEME);
    for (const base of BASES) expect(readViewState(JSON.stringify({ v: 1, base })).theme).toBe(DEFAULT_THEME);
    for (const theme of [null, 3, "sepia", "Dark", ""]) {
      expect(readViewState(JSON.stringify({ v: 1, base: OTHER, theme })), String(theme)).toEqual({ ...defaultViewState(), base: OTHER });
    }
    // A valid theme is kept beside an invalid neighbour.
    for (const theme of THEME_CHOICES) {
      expect(readViewState(JSON.stringify({ v: 9, base: "nowhere", theme })), theme).toEqual({ ...defaultViewState(), theme });
    }
  });

  it("the theme cycles Auto, Light, Dark, is applied to the root and stored with the mode", () => {
    const storage = memoryStorage();
    const store = new ViewStore(() => storage);
    expect(store.theme).toBe(DEFAULT_THEME);
    store.setBase(OTHER);
    const seen: (string | null)[] = [];
    for (let i = 0; i < THEME_CHOICES.length; i++) {
      store.cycleTheme();
      seen.push(document.documentElement.getAttribute("data-theme"));
      const next = new ViewStore(() => storage);
      expect(next.theme).toBe(store.theme);
      expect(next.base).toBe(OTHER);
    }
    // From Auto: Light, Dark, then Auto, which leaves the attribute off so
    // the system's scheme decides.
    expect(seen).toEqual(["light", "dark", null]);
    expect(store.theme).toBe("auto");
    // A mode change keeps the theme.
    store.cycleTheme();
    store.setBase(DEFAULT_BASE);
    expect(new ViewStore(() => storage).theme).toBe("light");
    applyTheme(DEFAULT_THEME);
  });

  it("the resolved theme is the choice, or the system's scheme on Auto", () => {
    for (const systemDark of [false, true]) {
      expect(resolveTheme("auto", systemDark)).toBe(systemDark ? "dark" : "light");
      expect(resolveTheme("light", systemDark)).toBe("light");
      expect(resolveTheme("dark", systemDark)).toBe("dark");
    }
  });

  it("a stored view without walking reads as walking on", () => {
    expect(DEFAULT_WALKING).toBe(true);
    for (const base of BASES) expect(readViewState(JSON.stringify({ v: 1, base }))).toEqual({ ...defaultViewState(), base });
    // An invalid value is the default; a valid one is kept.
    for (const walking of ["no", 0, null, {}]) expect(readViewState(JSON.stringify({ v: 1, walking })).walking).toBe(true);
    expect(readViewState(JSON.stringify({ v: 1, walking: false })).walking).toBe(false);
  });

  it("the walking setting is stored with the mode and the theme and read back", () => {
    const storage = memoryStorage();
    const store = new ViewStore(() => storage);
    expect(store.walking).toBe(DEFAULT_WALKING);
    store.setWalking(!DEFAULT_WALKING);
    store.toggleBase();
    store.cycleTheme();
    const next = new ViewStore(() => storage);
    expect([next.walking, next.base, next.theme]).toEqual([!DEFAULT_WALKING, OTHER, nextTheme(DEFAULT_THEME)]);
    applyTheme(DEFAULT_THEME);
    // An unchanged setting writes nothing.
    const quiet = memoryStorage();
    new ViewStore(() => quiet).setWalking(DEFAULT_WALKING);
    expect(quiet.items.has(VIEW_STORAGE_KEY)).toBe(false);
  });

  it("a stored view without focus reads as not focused", () => {
    expect(DEFAULT_FOCUS).toBe(false);
    for (const base of BASES) expect(readViewState(JSON.stringify({ v: 1, base })).focus).toBe(false);
    for (const focus of ["yes", 1, null, {}]) expect(readViewState(JSON.stringify({ v: 1, base: OTHER, focus })), String(focus)).toEqual({ ...defaultViewState(), base: OTHER });
    expect(readViewState(JSON.stringify({ v: 1, focus: true })).focus).toBe(true);
  });

  it("f turns Focus on over the base and off back to it; m leaves Focus for the other base", () => {
    const storage = memoryStorage();
    const store = new ViewStore(() => storage);
    for (const base of BASES) {
      store.setMode(base);
      store.toggleFocus();
      expect([store.mode, store.base, store.focus]).toEqual(["focus", base, true]);
      expect(new ViewStore(() => storage).mode).toBe("focus");
      store.toggleFocus();
      expect(store.mode).toBe(base);
      expect(new ViewStore(() => storage).mode).toBe(base);
    }
    store.setMode("focus");
    const hidden = store.base;
    store.toggleBase();
    expect([store.mode, store.focus]).toEqual([BASES.find((base) => base !== hidden), false]);
    // Choosing a base from Focus leaves Focus; choosing Focus again writes nothing.
    store.setMode("focus");
    store.setMode(hidden);
    expect(store.mode).toBe(hidden);
    store.setMode("focus");
    const quiet = memoryStorage();
    const unchanged = new ViewStore(() => quiet);
    unchanged.setMode(unchanged.mode);
    expect(quiet.items.has(VIEW_STORAGE_KEY)).toBe(false);
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
    // So does the theme, applied to the root; main.ts reads Auto.
    expect(store.theme).toBe(DEFAULT_THEME);
    store.cycleTheme();
    expect(store.theme).toBe(nextTheme(DEFAULT_THEME));
    expect(document.documentElement.getAttribute("data-theme")).toBe(store.theme);
    expect(storedViewState(() => throwing)).toEqual(defaultViewState());
    applyTheme(DEFAULT_THEME);
  });
});
