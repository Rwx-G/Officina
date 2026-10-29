// The scene's memory (AD-62 as amended, D-184): which project holds which
// office and which session holds which desk, kept on this machine, plus
// what only this run needs (last places, sticky spots, chip lists). One
// plain module variable, not reactive state, so a commit never re-runs the
// layout. Lost or unreadable storage costs stability only.
import type { Board } from "../generated/Board";
import type { Zone } from "../generated/Zone";
import type { Pose } from "./layout";
import { liveCards } from "./layout";
import type { Facing } from "./places";

export const STORAGE_KEY = "officina.scene.v2";

/** What survives a restart. */
export interface PersistedMemory {
  floor: string | null;
  /** Projects in office slot order, More offices' included. */
  offices: string[];
  /** Per project, the card key at each desk, or `null` for a free desk. */
  desks: Record<string, (string | null)[]>;
}

/** Where a live key stood on the last layout: kept while it is unknown. */
export interface LastPlace {
  spot: string | null;
  group: string | null;
  pose: Pose;
  facing: Facing;
}

export interface SceneMemory extends PersistedMemory {
  /** False until the run's first layout: `offices` then holds the previous
   *  run's order, pruned, and floor projects still come first. */
  inRun: boolean;
  /** Keys in their office's chip list, with the zone they joined it in. */
  chips: Record<string, Zone>;
  /** A first-free place kept while its key stays in that zone: reception,
   *  the lounge and an office's chip list; `null` is the zone's group. */
  sticky: Record<string, { zone: Zone; spot: string | null }>;
  last: Record<string, LastPlace>;
  /** The work orders already bound on the run's first layout that knew
   *  the bindings (D-568, D-836): never briefed, so a restart or a late
   *  Orchestrator view replays nothing. `null` until that layout. Kept for
   *  the run, never stored. */
  briefed: string[] | null;
}

export function emptyMemory(floor: string | null = null): SceneMemory {
  return { floor, offices: [], desks: {}, inRun: false, chips: {}, sticky: {}, last: {}, briefed: null };
}

/** The persisted part, serialised as `localStorage` holds it. */
export function serialise(memory: PersistedMemory): string {
  return JSON.stringify({ v: 2, floor: memory.floor, offices: memory.offices, desks: memory.desks });
}

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((v) => typeof v === "string");
}

/** A stored value read back, or `null` when it is not version 2's shape. */
export function parse(raw: string | null): PersistedMemory | null {
  if (raw === null) return null;
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return null;
  }
  if (typeof value !== "object" || value === null) return null;
  const v = value as Record<string, unknown>;
  if (v.v !== 2) return null;
  if (v.floor !== null && typeof v.floor !== "string") return null;
  if (!isStringArray(v.offices)) return null;
  if (typeof v.desks !== "object" || v.desks === null || Array.isArray(v.desks)) return null;
  const desks: Record<string, (string | null)[]> = {};
  for (const [project, list] of Object.entries(v.desks)) {
    if (!Array.isArray(list) || !list.every((k) => k === null || typeof k === "string")) return null;
    desks[project] = list as (string | null)[];
  }
  return { floor: v.floor, offices: v.offices, desks };
}

/** The stored memory made ready for the run's first layout: another
 *  floor's memory is discarded; offices without a live card on the first
 *  board, and desks whose key is not live in that project, are dropped. */
export function prune(stored: PersistedMemory | null, firstBoard: Board): SceneMemory {
  if (!stored || stored.floor !== firstBoard.floor) return emptyMemory(firstBoard.floor);
  const liveIn = new Map<string, Set<string>>();
  for (const card of liveCards(firstBoard)) {
    const keys = liveIn.get(card.project.name) ?? new Set<string>();
    keys.add(card.key);
    liveIn.set(card.project.name, keys);
  }
  const offices = [...new Set(stored.offices)].filter((project) => liveIn.has(project));
  const desks: Record<string, (string | null)[]> = {};
  for (const project of offices) {
    const live = liveIn.get(project)!;
    const kept = (stored.desks[project] ?? []).map((key) => (key !== null && live.has(key) ? key : null));
    if (kept.some((key) => key !== null)) desks[project] = kept;
  }
  return { ...emptyMemory(firstBoard.floor), offices, desks };
}

/** A storage that may be missing or throw on every access. */
export type StorageGetter = () => Pick<Storage, "getItem" | "setItem"> | null;

/** One memory per window: `load` once before the first layout, `commit`
 *  after each. A storage that throws leaves the memory working in place. */
export function sceneMemoryStore(getStorage: StorageGetter) {
  let memory: SceneMemory = emptyMemory();
  let loaded = false;
  let written: string | null = null;

  function storage() {
    try {
      return getStorage();
    } catch {
      return null;
    }
  }

  return {
    /** Reads storage the first time only; later calls return the memory. */
    load(firstBoard: Board): SceneMemory {
      if (loaded) return memory;
      loaded = true;
      let raw: string | null;
      try {
        raw = storage()?.getItem(STORAGE_KEY) ?? null;
      } catch {
        raw = null;
      }
      written = raw;
      memory = prune(parse(raw), firstBoard);
      return memory;
    },
    current(): SceneMemory {
      return memory;
    },
    /** Keeps `next`; writes storage only when its persisted part changed. */
    commit(next: SceneMemory): void {
      memory = next;
      const text = serialise(next);
      if (text === written) return;
      try {
        storage()?.setItem(STORAGE_KEY, text);
        written = text;
      } catch {
        // Unwritable storage: the next commit tries again.
      }
    },
  };
}

const store = sceneMemoryStore(() => globalThis.localStorage ?? null);

export const load = store.load;
export const current = store.current;
export const commit = store.commit;
