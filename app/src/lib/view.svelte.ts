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

/** The Sessions view's bases: what Focus hides and `f` brings back. */
export type Base = "board" | "scene";

export const BASES: readonly Base[] = ["board", "scene"];

/** The Sessions view's modes: a base, or Focus over it (AD-66). */
export type Mode = Base | "focus";

/** Board until the owner has chosen Scene for ten working days (D-481). */
export const DEFAULT_BASE: Base = "board";

/** Figures walk to their places (D-469); story 4.14's AD-71 walking run
 *  turns this off if walking costs more than FR-9 allows (D-480, D-515). */
export const DEFAULT_WALKING = true;

/** Focus is off at start until the author turns it on (D-113). */
export const DEFAULT_FOCUS = false;

export const VIEW_STORAGE_KEY = "officina.view.v1";

export interface ViewState {
  v: 1;
  base: Base;
  /** Auto follows the system (AD-67). */
  theme: ThemeChoice;
  walking: boolean;
  /** Focus hides the base, which it keeps (AD-66). */
  focus: boolean;
}

export function defaultViewState(): ViewState {
  return { v: 1, base: DEFAULT_BASE, theme: DEFAULT_THEME, walking: DEFAULT_WALKING, focus: DEFAULT_FOCUS };
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
  if (typeof fields.walking === "boolean") state.walking = fields.walking;
  if (typeof fields.focus === "boolean") state.focus = fields.focus;
  return state;
}

export function serialiseViewState(state: ViewState): string {
  return JSON.stringify({ v: state.v, base: state.base, theme: state.theme, walking: state.walking, focus: state.focus });
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
  /** The header's Walking toggle, shown in Scene mode (AD-66). */
  walking = $state(DEFAULT_WALKING);
  focus = $state(DEFAULT_FOCUS);
  /** What the Sessions view shows. */
  mode = $derived<Mode>(this.focus ? "focus" : this.base);

  readonly #storage: ViewStorageGetter;

  constructor(storage: ViewStorageGetter = browserStorage) {
    this.#storage = storage;
    const stored = storedViewState(storage);
    this.base = stored.base;
    this.theme = stored.theme;
    this.walking = stored.walking;
    this.focus = stored.focus;
    globalThis.matchMedia?.(DARK_SCHEME_QUERY).addEventListener("change", (event) => (this.systemDark = event.matches));
  }

  #store(): void {
    try {
      this.#storage()?.setItem(
        VIEW_STORAGE_KEY,
        serialiseViewState({ v: 1, base: this.base, theme: this.theme, walking: this.walking, focus: this.focus }),
      );
    } catch {
      // An unwritable storage keeps the choice for this run only.
    }
  }

  /** A mode from the mode switch: a base leaves Focus, Focus keeps the
   *  base it hides. */
  setMode(mode: Mode): void {
    const focus = mode === "focus";
    const base = mode === "focus" ? this.base : mode;
    if (focus === this.focus && base === this.base) return;
    this.focus = focus;
    this.base = base;
    this.#store();
  }

  setBase(base: Base): void {
    this.setMode(base);
  }

  setWalking(walking: boolean): void {
    if (walking === this.walking) return;
    this.walking = walking;
    this.#store();
  }

  /** `m`: the other base, leaving Focus. */
  toggleBase(): void {
    this.setBase(this.base === "board" ? "scene" : "board");
  }

  /** `f`: Focus, or back to the base it hides. */
  toggleFocus(): void {
    this.setMode(this.focus ? this.base : "focus");
  }

  /** The theme control: the next of Auto, Light and Dark, applied to the
   *  root at once (`main.ts` applies the stored one before mounting). */
  cycleTheme(): void {
    this.theme = nextTheme(this.theme);
    applyTheme(this.theme);
    this.#store();
  }
}
