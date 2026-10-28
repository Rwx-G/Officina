// What the window shows (AD-66, D-113): the view, local to the window and
// never stored, and the stored `ViewState`, whose fields are read one by one
// with their defaults, so a field added later needs no version bump and an
// unreadable value or a storage that throws gives the defaults.

import {
  DARK_SCHEME_QUERY,
  DEFAULT_THEME,
  applyTheme,
  isThemeChoice,
  nextTheme,
  resolveTheme,
  systemPrefersDark,
  type ThemeChoice,
} from "./theme";

/** The window's views: local state, never part of the Rust contract. */
export type WindowView = "sessions" | "subjects" | "relay";

/** The Sessions view's modes. */
export type Base = "board" | "scene";

export const BASES: readonly Base[] = ["board", "scene"];

/** Board until the owner has chosen Scene for ten working days (D-481). */
export const DEFAULT_BASE: Base = "board";

export const VIEW_STORAGE_KEY = "officina.view.v1";

export interface ViewState {
  v: 1;
  base: Base;
  /** Auto follows the system (AD-67). */
  theme: ThemeChoice;
}

export function defaultViewState(): ViewState {
  return { v: 1, base: DEFAULT_BASE, theme: DEFAULT_THEME };
}

function isBase(value: unknown): value is Base {
  return BASES.includes(value as Base);
}

/** A stored value read field by field: each field missing or invalid takes
 *  its default, the others are kept. */
export function readViewState(raw: string | null): ViewState {
  const state = defaultViewState();
  if (raw === null) return state;
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return state;
  }
  if (typeof value !== "object" || value === null) return state;
  const fields = value as Record<string, unknown>;
  if (isBase(fields.base)) state.base = fields.base;
  if (isThemeChoice(fields.theme)) state.theme = fields.theme;
  return state;
}

export function serialiseViewState(state: ViewState): string {
  return JSON.stringify({ v: state.v, base: state.base, theme: state.theme });
}

/** A storage that may be missing or throw on every access. */
export type ViewStorageGetter = () => Pick<Storage, "getItem" | "setItem"> | null;

const browserStorage: ViewStorageGetter = () => globalThis.localStorage ?? null;

/** What the storage holds, or the defaults when it cannot be read. */
export function storedViewState(storage: ViewStorageGetter = browserStorage): ViewState {
  let raw: string | null;
  try {
    raw = storage()?.getItem(VIEW_STORAGE_KEY) ?? null;
  } catch {
    raw = null;
  }
  return readViewState(raw);
}

export class ViewStore {
  /** Sessions at start (D-113). */
  view = $state<WindowView>("sessions");
  base = $state<Base>(DEFAULT_BASE);
  theme = $state<ThemeChoice>(DEFAULT_THEME);
  /** The system's scheme, followed live for Auto. */
  systemDark = $state(systemPrefersDark());
  /** The theme the window shows. */
  resolvedTheme = $derived(resolveTheme(this.theme, this.systemDark));

  readonly #storage: ViewStorageGetter;

  constructor(storage: ViewStorageGetter = browserStorage) {
    this.#storage = storage;
    const stored = storedViewState(storage);
    this.base = stored.base;
    this.theme = stored.theme;
    globalThis.matchMedia?.(DARK_SCHEME_QUERY).addEventListener("change", (event) => (this.systemDark = event.matches));
  }

  #store(): void {
    try {
      this.#storage()?.setItem(VIEW_STORAGE_KEY, serialiseViewState({ v: 1, base: this.base, theme: this.theme }));
    } catch {
      // An unwritable storage keeps the choice for this run only.
    }
  }

  setBase(base: Base): void {
    if (base === this.base) return;
    this.base = base;
    this.#store();
  }

  /** `m`: the other mode. */
  toggleBase(): void {
    this.setBase(this.base === "board" ? "scene" : "board");
  }

  /** The theme control: the next of Auto, Light and Dark, applied to the
   *  root at once (`main.ts` applies the stored one before mounting). */
  cycleTheme(): void {
    this.theme = nextTheme(this.theme);
    applyTheme(this.theme);
    this.#store();
  }
}
