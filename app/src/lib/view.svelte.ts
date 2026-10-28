// What the window shows (AD-66, D-113): the view, local to the window and
// never stored, and the stored `ViewState`, whose fields are read one by one
// with their defaults, so a field added later needs no version bump and an
// unreadable value or a storage that throws gives the defaults.

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
}

export function defaultViewState(): ViewState {
  return { v: 1, base: DEFAULT_BASE };
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
  return state;
}

export function serialiseViewState(state: ViewState): string {
  return JSON.stringify({ v: state.v, base: state.base });
}

/** A storage that may be missing or throw on every access. */
export type ViewStorageGetter = () => Pick<Storage, "getItem" | "setItem"> | null;

const browserStorage: ViewStorageGetter = () => globalThis.localStorage ?? null;

export class ViewStore {
  /** Sessions at start (D-113). */
  view = $state<WindowView>("sessions");
  base = $state<Base>(DEFAULT_BASE);

  readonly #storage: ViewStorageGetter;

  constructor(storage: ViewStorageGetter = browserStorage) {
    this.#storage = storage;
    let raw: string | null;
    try {
      raw = this.#storage()?.getItem(VIEW_STORAGE_KEY) ?? null;
    } catch {
      raw = null;
    }
    this.base = readViewState(raw).base;
  }

  setBase(base: Base): void {
    if (base === this.base) return;
    this.base = base;
    try {
      this.#storage()?.setItem(VIEW_STORAGE_KEY, serialiseViewState({ v: 1, base }));
    } catch {
      // An unwritable storage keeps the mode for this run only.
    }
  }

  /** `m`: the other mode. */
  toggleBase(): void {
    this.setBase(this.base === "board" ? "scene" : "board");
  }
}
