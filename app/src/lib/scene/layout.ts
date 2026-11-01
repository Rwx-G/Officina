// The scene's layout (AD-60 as amended): one pure function from the board
// and the scene's memory to places, poses, marks, desks, plaques, groups,
// targets and words. No DOM, clock, storage, flag or randomness: an
// identical input gives a deep-equal layout, and the layout is a fixed
// point of the memory it returns (D-102). It names places, never motion.
import type { AlertCause } from "../generated/AlertCause";
import type { Board } from "../generated/Board";
import type { Card } from "../generated/Card";
import type { LiveStatus } from "../generated/LiveStatus";
import type { OrchestratorPresence } from "../generated/OrchestratorPresence";
import type { OrchestratorView } from "../generated/OrchestratorView";
import type { ProjectView } from "../generated/ProjectView";
import { proposalLabel } from "../orchestrator-text";
import type { Zone } from "../generated/Zone";
import {
  MAX_OFFICES,
  SHARED_CELLS,
  canvas,
  centreOf,
  chipTarget,
  columnsFor,
  figureTarget,
  intersects,
  moreOfficesTarget,
  officeCells,
  projector,
  zoneGroupTarget,
  type Cell,
  type Point,
  type Rect,
  type RoomKind,
  type SharedZone,
} from "./geometry";
import type { LastPlace, SceneMemory } from "./memory";
import { CAPACITY, officeSpots, restPoint, restPointFor, sharedSpots, staffSpots, type Facing, type Spot } from "./places";
import { figureWords, groupWords, helperWords, orchestratorWords, plaqueText, summaryText, type FigureFacts } from "./text";

/** What the orchestrator proposed (story 5.17): how many proposals are
 *  pending, and the labels of the latest, newest first, each the
 *  orchestrator's or a session's text (D-831). */
export interface SceneProposals {
  pending: number;
  latest: string[];
}

/** A work order bound to its session (AD-81), as the Orchestrator view
 *  lists it (story 5.8): what a briefing is drawn from (D-568). */
export interface SceneBriefing {
  orderId: string;
  sessionId: string;
  boundAtMs: number;
}

/** The most proposal labels the whiteboard shows (D-567). */
export const MAX_LABELS = 4;

/** What the scene takes of the Orchestrator view's proposals (D-831): the
 *  pending count, and the labels of the latest pending ones, newest first
 *  as the view lists them. A session is named as its card names it. */
export function sceneProposals(view: OrchestratorView, board: Board): SceneProposals {
  const pending = view.proposals.filter((p) => p.status === "pending");
  const names = new Map<string, string>();
  for (const card of Object.values(board.columns).flat()) if (card.session_id !== null) names.set(card.session_id, card.name);
  const subjects = new Map(view.briefing.subjects.items.map((s) => [s.id, s.name]));
  return {
    pending: pending.length,
    latest: pending.flatMap((p) => (p.content ? [proposalLabel(p.content, names, subjects)] : [])).slice(0, MAX_LABELS),
  };
}

/** The Orchestrator view's bound work orders (AD-81, D-568). */
export function sceneBriefings(view: OrchestratorView): SceneBriefing[] {
  return view.work_orders.flatMap((o) =>
    o.bound_at_ms !== null && o.session !== null ? [{ orderId: o.id, sessionId: o.session.session_id, boundAtMs: o.bound_at_ms }] : [],
  );
}

export interface SceneInput {
  board: Board;
  /** The card key of the open meeting (D-566): its figure is marked
   *  received wherever it stands; absent or `null`, nobody is. */
  received?: string | null;
  /** The orchestrator while `claude agents` lists it (`Board.orchestrator`,
   *  story 5.16): drawn at the decider's chair. Absent or `null`, the chair
   *  stays empty, as v0.4 draws it. */
  orchestrator?: OrchestratorPresence | null;
  /** Absent or `null`, the screens show none pending and the whiteboard
   *  no label. */
  proposals?: SceneProposals | null;
  /** The bound work orders. Absent or `null` while the Orchestrator view
   *  is not known yet: then no order is briefed, and the first layout that
   *  knows them takes every one as already briefed (D-836). */
  briefings?: SceneBriefing[] | null;
}

/** Which of a figure's pose groups shows (AD-171). */
export type Pose = "standing" | "seated-facing" | "seated-working";

export interface Marks {
  /** The board number, 1 to 9 only (D-107). */
  ticket: number | null;
  /** An approval-only wait: an amber ticket without a digit (D-464). */
  approval: boolean;
  /** The desk, 1 for an office's first, of a Your turn session that has
   *  one: drawn as its letter on a square tag (D-558). */
  deskBadge: number | null;
  hand: boolean;
  ring: boolean;
  question: boolean;
  /** The open meeting's figure (D-566): a ring under it in the working
   *  colour, wherever it stands; never on a figure inside a group. */
  received: boolean;
}

export interface Employee {
  key: string;
  zone: Zone;
  project: string;
  /** The spot the figure stands or sits at, or `null` inside a group. */
  spot: string | null;
  /** The group listing it, when it has no spot. */
  group: string | null;
  /** Its resting floor point on screen, when it has a spot. */
  at: Point | null;
  pose: Pose;
  facing: Facing;
  marks: Marks;
  alert: AlertCause | null;
  /** A work order bound to this session since the orchestrator came, not
   *  yet briefed (D-568): the mover detours it by the counter once. Its
   *  place and target stay its final ones (D-471). */
  briefing: { orderId: string } | null;
}

/** The orchestrator at the decider's chair (AD-179, D-567): never a card,
 *  never counted, ringed, numbered or grouped. */
export interface Staff {
  /** Its figure's key, which no card key can be. */
  key: string;
  spot: string;
  /** Its resting floor point on screen. */
  at: Point;
  pose: Pose;
  facing: Facing;
  status: LiveStatus | null;
}

/** Whose figure a rig draws: a session's or a sub-agent's, or the
 *  orchestrator's (AD-179, D-567). */
export type Role = "employee" | "orchestrator";

/** The figure key of the orchestrator: card keys start `pid:` or `sid:`. */
export const ORCHESTRATOR_KEY = "officina:orchestrator";

/** What the decider's screens and the whiteboard show while the
 *  orchestrator is live (D-567, D-830): the pending proposals on the left
 *  screen, the Needs you count on the right, the latest labels on the
 *  whiteboard, newest first. */
export interface DecisionDisplay {
  proposals: number;
  needsYou: number;
  labels: string[];
}

/** A running sub-agent drawn beside its lead's desk (AD-176, D-476). */
export interface Helper {
  /** The lead's key and the sub-agent's id: a resumed sub-agent keeps it
   *  (D-547). */
  key: string;
  lead: string;
  project: string;
  /** Its floor point on the grid, in tiles. */
  gx: number;
  gy: number;
  /** Its floor point on screen. */
  at: Point;
}

/** Where a lead's first sub-agents stand, relative to its desk's chair, in
 *  tiles (the prototype v2's `spawnHelper`): no more are drawn. */
export const HELPER_OFFSETS: readonly (readonly [dgx: number, dgy: number])[] = [
  [-0.62, 0.42],
  [0.62, 0.42],
  [0, 0.78],
];

/** A sub-agent's figure is a session's scaled by this, without a cap
 *  (D-477). */
export const SUBAGENT_SCALE = 0.68;

/** A sub-agent figure's key: unique on the board, stable while it runs. */
export const helperKey = (lead: string, agentId: string): string => `${lead}/agent:${agentId}`;

export type DeskState = "free" | "occupied" | "kept";

export interface Desk {
  slot: number;
  index: number;
  spot: string;
  state: DeskState;
  key: string | null;
  /** On a kept desk's empty chair: the board number while its session
   *  waits, its own desk (drawn as a letter, D-558) while it is in the
   *  lounge. */
  number: number | null;
  /** The screen is lit while its session works there. */
  lit: boolean;
}

export type PlaqueState = "quiet" | "ink" | "attention" | "alert" | "dimmed";

export interface Plaque {
  text: string;
  count: number;
  state: PlaqueState;
  highPriority: boolean;
  /** The size of the room's group, drawn as "+n"; 0 without one. */
  more: number;
}

export interface Room {
  id: string;
  kind: RoomKind | "more";
  cell: Cell;
  slot: number | null;
  /** An office's project; `null` for a shared room and More offices. */
  project: ProjectView | null;
  /** More offices' projects, in slot order; empty elsewhere. */
  projects: string[];
  plaque: Plaque;
}

export interface Group {
  id: string;
  room: string;
  keys: string[];
}

/** A figure's target opens its session's file, a sub-agent's (`helper`)
 *  its lead's, a group's lists its members; the live orchestrator's opens
 *  the Orchestrator view, its empty chair's (`chair`) the orchestrator's
 *  file, which offers Start (D-567). */
export type TargetKind = "figure" | "helper" | "group" | "orchestrator" | "chair";

export interface Target {
  id: string;
  /** The tab group arrows stay in (AD-63): a room's id. */
  group: string;
  kind: TargetKind;
  rect: Rect;
  label: string;
  tag: string;
  /** What the name tag adds after the name, set apart: a figure's project,
   *  a sub-agent's lead (D-1044); `null` for none. */
  context: string | null;
  keys: string[];
}

export interface SceneLayout {
  columns: number;
  canvas: { w: number; h: number };
  rooms: Room[];
  employees: Employee[];
  /** The sub-agents drawn, by lead in board order, then by start. */
  helpers: Helper[];
  desks: Desk[];
  groups: Group[];
  targets: Target[];
  /** The orchestrator, while it is live. */
  orchestrator: Staff | null;
  /** The screens' and the whiteboard's words, while it is live. */
  display: DecisionDisplay | null;
  summary: string;
  memory: SceneMemory;
}

/** Needs you, Your turn and Working: the cards the scene draws (D-102). */
export function liveCards(board: Board): Card[] {
  const seen = new Set<string>();
  const out: Card[] = [];
  for (const card of [...board.columns.needs_you, ...board.columns.your_turn, ...board.columns.working]) {
    if (seen.has(card.key)) continue;
    seen.add(card.key);
    out.push(card);
  }
  return out;
}

/** D-102's ties: earliest start (`null` last), then project, then key. */
function byStart(a: Card, b: Card): number {
  const sa = a.started_at_ms ?? Number.POSITIVE_INFINITY;
  const sb = b.started_at_ms ?? Number.POSITIVE_INFINITY;
  if (sa !== sb) return sa < sb ? -1 : 1;
  return compareText(a.project.name, b.project.name) || compareText(a.key, b.key);
}

function compareText(a: string, b: string): number {
  return a < b ? -1 : a > b ? 1 : 0;
}

function unique(list: string[]): string[] {
  return [...new Set(list)];
}

/** A card's zone; a live card without a scene state is missing data. */
export function zoneOf(card: Card): Zone {
  return card.scene?.zone ?? "unknown";
}

const MORE = "more";
const officeId = (slot: number) => `office${slot}`;

/** The highest board number a key carries (D-107). */
export const MAX_NUMBER = 9;

export function layoutScene(input: SceneInput, memory: SceneMemory): SceneLayout {
  const { board } = input;
  const received = input.received ?? null;
  const presence = input.orchestrator ?? null;
  const proposals = input.proposals ?? null;
  const live = liveCards(board);
  const cardOf = new Map(live.map((c) => [c.key, c]));

  // Briefings (D-568, D-836): an order bound while the orchestrator is
  // live, after it came, and not already bound when the run's layouts
  // first knew the bindings.
  const known = input.briefings ?? null;
  const briefed = memory.briefed ?? (known ? known.map((b) => b.orderId) : null);
  const briefingOf = new Map<string, string>();
  if (presence && known && briefed) {
    const done = new Set(briefed);
    for (const b of known) {
      if (done.has(b.orderId) || (presence.started_at_ms !== null && b.boundAtMs < presence.started_at_ms)) continue;
      if (!briefingOf.has(b.sessionId)) briefingOf.set(b.sessionId, b.orderId);
    }
  }

  // Offices (AD-62): floor projects in file order, then remembered ones
  // with a live card, then first appearance; kept until restart.
  const floorNames = unique(board.floor_projects.map((p) => p.name));
  const byAppearance = unique([...live].sort(byStart).map((c) => c.project.name));
  const liveProjects = new Set(byAppearance);
  const order = memory.inRun
    ? unique([...memory.offices, ...floorNames, ...byAppearance])
    : unique([...floorNames, ...memory.offices.filter((p) => liveProjects.has(p)), ...byAppearance]);
  const columns = columnsFor(order.length);
  const overflow = order.length > MAX_OFFICES;
  const regular = overflow ? order.slice(0, MAX_OFFICES - 1) : order;
  const moreProjects = overflow ? order.slice(MAX_OFFICES - 1) : [];
  const slotOf = new Map(regular.map((p, slot) => [p, slot]));
  const spotsOf = regular.map((_, slot) => officeSpots(slot));

  const numberOf = new Map<string, number>();
  board.columns.needs_you.forEach((c, i) => {
    if (i < MAX_NUMBER) numberOf.set(c.key, i + 1);
  });

  const spotById = new Map<string, Spot>();
  for (const zone of Object.keys(SHARED_CELLS) as SharedZone[]) for (const s of sharedSpots(zone)) spotById.set(s.id, s);
  for (const { desks: d, chips: c } of spotsOf) for (const s of [...d, ...c]) spotById.set(s.id, s);
  /** An unknown key's last place, when it still names a spot of this floor. */
  const kept = (key: string): LastPlace | undefined => {
    const last = memory.last[key];
    if (!last) return undefined;
    if (last.spot !== null && !spotById.has(last.spot)) return undefined;
    return last;
  };

  // Desks (AD-62, D-102): kept while live; the lowest free one the first
  // time a session is laid out outside reception; else the chip list,
  // until the session's own zone changes.
  const desks = new Map<string, (string | null)[]>();
  for (const project of regular) {
    const remembered = memory.desks[project] ?? [];
    desks.set(
      project,
      Array.from({ length: CAPACITY.desks }, (_, d) => {
        const key = remembered[d] ?? null;
        return key !== null && cardOf.get(key)?.project.name === project ? key : null;
      }),
    );
  }
  const deskOf = new Map<string, number>();
  for (const [, list] of desks) list.forEach((key, d) => key !== null && deskOf.set(key, d));
  const chips: Record<string, Zone> = {};
  for (const card of [...live].sort(byStart)) {
    const list = desks.get(card.project.name);
    if (!list || deskOf.has(card.key)) continue;
    const zone = zoneOf(card);
    const chipZone = memory.chips[card.key];
    if (zone === "unknown" && kept(card.key)) {
      if (chipZone !== undefined) chips[card.key] = chipZone;
      continue;
    }
    if (zone === "reception") continue;
    if (chipZone === zone) {
      chips[card.key] = chipZone;
      continue;
    }
    const free = list.indexOf(null);
    if (free >= 0) {
      list[free] = card.key;
      deskOf.set(card.key, free);
    } else {
      chips[card.key] = zone;
    }
  }

  // Places (AD-62, AD-167): unknown keeps its last place, and the spots it
  // holds are taken; the queue and the alert room follow board order;
  // reception, the lounge and the chip list keep their first free spot.
  const place = new Map<string, { spot: Spot | null; group: string | null; pose: Pose; facing: Facing }>();
  const taken = new Set<string>();
  for (const card of live) {
    if (zoneOf(card) !== "unknown") continue;
    const last = kept(card.key);
    if (!last) continue;
    const spot = last.spot === null ? null : spotById.get(last.spot)!;
    place.set(card.key, { spot, group: last.group, pose: last.pose, facing: last.facing });
    if (spot) taken.add(spot.id);
  }

  // Board order (AD-62): Needs you in `columns.needs_you` order, then the
  // approval-only waits by their earliest valid request (D-464, D-496).
  const approvalAt = (card: Card) => {
    const times = board.approvals
      .filter((a) => a.valid && a.session_id !== null && a.session_id === card.session_id)
      .map((a) => a.requested_at_ms);
    return times.length ? Math.min(...times) : (card.scene?.waiting_since_ms ?? Number.POSITIVE_INFINITY);
  };
  const boardOrder = (zone: "decision" | "alert") => {
    const numbered = live.filter((c) => c.column === "needs_you" && zoneOf(c) === zone);
    const approvals = live
      .filter((c) => c.column !== "needs_you" && zoneOf(c) === zone)
      .sort((a, b) => approvalAt(a) - approvalAt(b) || compareText(a.key, b.key));
    return [...numbered, ...approvals];
  };
  for (const zone of ["decision", "alert"] as const) {
    const free = sharedSpots(zone).filter((s) => !taken.has(s.id));
    boardOrder(zone).forEach((card, i) => {
      const spot = free[i] ?? null;
      place.set(card.key, { spot, group: spot ? null : zone, pose: "standing", facing: spot?.facing ?? "front" });
      if (spot) taken.add(spot.id);
    });
  }

  const sticky: SceneMemory["sticky"] = {};
  /** A kept place while the key stays in `zone` (its group included),
   *  else the first free spot, else the group (`null`). */
  const stickyPlace = (card: Card, zone: Zone, spots: Spot[]): string | null => {
    const previous = memory.sticky[card.key];
    if (previous && previous.zone === zone && (previous.spot === null || spots.some((s) => s.id === previous.spot))) {
      return previous.spot;
    }
    return spots.find((s) => !taken.has(s.id))?.id ?? null;
  };
  // Kept sticky places first, so a newcomer never takes one. A chip list
  // member that has just taken a desk leaves its chip spot free.
  for (const card of live) {
    const previous = memory.sticky[card.key];
    const leftForDesk = previous?.zone === "office" && deskOf.has(card.key);
    if (previous?.spot && previous.zone === zoneOf(card) && !leftForDesk) taken.add(previous.spot);
    if (previous && zoneOf(card) === "unknown") sticky[card.key] = previous;
  }

  const officePlace = (card: Card, pose: Pose) => {
    const slot = slotOf.get(card.project.name);
    if (slot === undefined) return { spot: null, group: MORE, pose: "standing" as Pose, facing: "front" as Facing };
    const d = deskOf.get(card.key);
    if (d !== undefined) {
      const spot = spotsOf[slot]!.desks[d]!;
      return { spot, group: null, pose, facing: pose === "seated-working" ? spot.facing : ("front" as Facing) };
    }
    const chipSpots = spotsOf[slot]!.chips;
    const id = stickyPlace(card, "office", chipSpots);
    sticky[card.key] = { zone: "office", spot: id };
    if (id) taken.add(id);
    const spot = id ? spotById.get(id)! : null;
    return { spot, group: spot ? null : officeId(slot), pose: "standing" as Pose, facing: "front" as Facing };
  };

  for (const card of live) {
    if (place.has(card.key)) continue;
    const zone = zoneOf(card);
    if (zone === "reception" || zone === "lounge") {
      const spots = sharedSpots(zone);
      const id = stickyPlace(card, zone, spots);
      sticky[card.key] = { zone, spot: id };
      if (id) taken.add(id);
      const spot = id ? spotById.get(id)! : null;
      const pose: Pose = zone === "lounge" && spot?.posture === "seated" ? "seated-facing" : "standing";
      place.set(card.key, { spot, group: spot ? null : zone, pose, facing: spot?.facing ?? (zone === "reception" ? "back" : "front") });
    } else if (zone === "office") {
      place.set(card.key, officePlace(card, "seated-working"));
    } else if (zone === "unknown") {
      place.set(card.key, officePlace(card, "seated-facing"));
    }
  }

  // Employees, in board order.
  const p = projector(columns);
  const employees: Employee[] = live.map((card) => {
    const zone = zoneOf(card);
    const at = place.get(card.key)!;
    const number = numberOf.get(card.key) ?? null;
    const desk = deskOf.get(card.key);
    const waiting = zone === "decision" || zone === "alert";
    const marks: Marks = {
      ticket: (waiting || zone === "unknown") ? number : null,
      approval: waiting && number === null && card.scene?.approval === true,
      deskBadge: zone === "lounge" && desk !== undefined ? desk + 1 : null,
      hand: zone === "lounge",
      ring: zone === "lounge",
      question: zone === "unknown",
      received: card.key === received && at.spot !== null,
    };
    const rest = at.spot ? restPointFor(at.spot, card.key) : null;
    const orderId = card.session_id !== null ? briefingOf.get(card.session_id) : undefined;
    return {
      key: card.key,
      zone,
      project: card.project.name,
      spot: at.spot?.id ?? null,
      group: at.group,
      at: rest ? p(rest.gx, rest.gy) : null,
      pose: at.pose,
      facing: at.facing,
      marks,
      alert: card.scene?.alert ?? null,
      briefing: orderId !== undefined ? { orderId } : null,
    };
  });

  // The orchestrator (D-567): seated at the decider's chair, at its desk
  // while busy, facing the room otherwise; a waiting one waits on nobody.
  const chair = staffSpots().chair;
  const chairRest = restPoint(chair);
  const chairAt = p(chairRest.gx, chairRest.gy);
  const orchestrator: Staff | null = presence
    ? {
        key: ORCHESTRATOR_KEY,
        spot: chair.id,
        at: chairAt,
        pose: presence.status === "busy" ? "seated-working" : "seated-facing",
        facing: presence.status === "busy" ? chair.facing : "front",
        status: presence.status,
      }
    : null;
  const display: DecisionDisplay | null = presence
    ? { proposals: proposals?.pending ?? 0, needsYou: board.waiting_count, labels: (proposals?.latest ?? []).slice(0, MAX_LABELS) }
    : null;
  const employeeOf = new Map(employees.map((e) => [e.key, e]));
  const bySpot = new Map(employees.filter((e) => e.spot !== null).map((e) => [e.spot!, e]));

  // Desks.
  const deskList: Desk[] = [];
  regular.forEach((project, slot) => {
    desks.get(project)!.forEach((key, index) => {
      const spot = spotsOf[slot]!.desks[index]!.id;
      const owner = key !== null ? employeeOf.get(key) : undefined;
      const occupied = owner?.spot === spot;
      let number: number | null = null;
      if (owner && !occupied) {
        if (owner.zone === "decision" || owner.zone === "alert") number = owner.marks.ticket;
        else if (owner.zone === "lounge") number = index + 1;
      }
      deskList.push({
        slot,
        index,
        spot,
        state: key === null ? "free" : occupied ? "occupied" : "kept",
        key,
        number,
        lit: occupied && owner?.zone === "office",
      });
    });
  });

  // Sub-agents beside their lead's desk, wherever the lead stands (AD-176):
  // a lead without a desk shows them in its file only.
  const helpers: Helper[] = [];
  for (const e of employees) {
    const slot = slotOf.get(e.project);
    const d = deskOf.get(e.key);
    if (slot === undefined || d === undefined) continue;
    const chair = spotsOf[slot]!.desks[d]!;
    cardOf.get(e.key)!.subagents.slice(0, HELPER_OFFSETS.length).forEach((agent, i) => {
      const [dgx, dgy] = HELPER_OFFSETS[i]!;
      const gx = chair.gx + dgx;
      const gy = chair.gy + dgy;
      helpers.push({ key: helperKey(e.key, agent.agent_id), lead: e.key, project: e.project, gx, gy, at: p(gx, gy) });
    });
  }

  // Each group's members in board order (D-484).
  const groupKeys = new Map<string, string[]>();
  for (const e of employees) {
    if (e.group === null) continue;
    groupKeys.set(e.group, [...(groupKeys.get(e.group) ?? []), e.key]);
  }

  // Rooms and plaques.
  const projectView = (name: string): ProjectView =>
    board.floor_projects.find((p) => p.name === name) ??
    live.find((c) => c.project.name === name)?.project ??
    [...board.columns.to_resume, ...board.columns.done].find((c) => c.project.name === name)?.project ?? {
      name,
      color: null,
      priority: "normal",
    };
  const count = (f: (e: Employee) => boolean) => employees.filter(f).length;
  const sharedState: Record<SharedZone, (n: number) => PlaqueState> = {
    reception: () => "quiet",
    decision: (n) => (n > 0 ? "attention" : "quiet"),
    alert: (n) => (n > 0 ? "alert" : "quiet"),
    lounge: () => "quiet",
  };
  // The alert room counts whoever stands in it: an unknown employee kept
  // at its last place there keeps its alert (D-494), so the room stays lit.
  const alertSpots = new Set(sharedSpots("alert").map((s) => s.id));
  const inAlertRoom = (e: Employee) => e.group === "alert" || (e.spot !== null && alertSpots.has(e.spot));
  const occupies: Record<SharedZone, (e: Employee) => boolean> = {
    reception: (e) => e.zone === "reception",
    decision: (e) => e.zone === "decision",
    alert: (e) => e.zone === "alert" || (e.zone === "unknown" && inAlertRoom(e)),
    lounge: (e) => e.zone === "lounge",
  };
  const rooms: Room[] = (Object.keys(SHARED_CELLS) as SharedZone[]).map((zone) => {
    const n = count(occupies[zone]);
    return {
      id: zone,
      kind: zone,
      cell: SHARED_CELLS[zone],
      slot: null,
      project: null,
      projects: [],
      plaque: { text: plaqueText(zone, presence !== null), count: n, state: sharedState[zone](n), highPriority: false, more: groupKeys.get(zone)?.length ?? 0 },
    };
  });
  const cells = officeCells(columns);
  regular.forEach((project, slot) => {
    const view = projectView(project);
    const n = count((e) => e.project === project);
    rooms.push({
      id: officeId(slot),
      kind: "office",
      cell: cells[slot]!,
      slot,
      project: view,
      projects: [],
      plaque: { text: project, count: n, state: n > 0 ? "ink" : "dimmed", highPriority: view.priority === "high", more: groupKeys.get(officeId(slot))?.length ?? 0 },
    });
  });
  if (overflow) {
    const n = count((e) => moreProjects.includes(e.project));
    rooms.push({
      id: MORE,
      kind: "more",
      cell: cells[MAX_OFFICES - 1]!,
      slot: MAX_OFFICES - 1,
      project: null,
      projects: moreProjects,
      plaque: { text: plaqueText(MORE), count: moreProjects.length, state: n > 0 ? "ink" : "dimmed", highPriority: moreProjects.some((m) => projectView(m).priority === "high"), more: 0 },
    });
  }

  // Targets in tab order (AD-63): reception, the queue and its register,
  // the orchestrator or its empty chair, the alert room, offices by slot
  // (desks front to back, chip spots, the chip), the lounge, More offices.
  // A group names its room as v0.4 did: the queue's members wait on the
  // author, whatever the plaque says (R110).
  const facts = (e: Employee): FigureFacts => ({ card: cardOf.get(e.key)!, employee: e });
  const targets: Target[] = [];
  const figure = (spot: Spot, group: string) => {
    const e = bySpot.get(spot.id);
    if (!e || !e.at) return;
    const words = figureWords(facts(e));
    targets.push({ id: `figure:${e.key}`, group, kind: "figure", rect: figureTarget(e.at), label: words.label, tag: words.tag, context: words.context, keys: [e.key] });
  };
  const group = (id: string, room: string, rect: Rect, roomName: string) => {
    const keys = groupKeys.get(id) ?? [];
    if (keys.length === 0 && id !== MORE) return;
    const words = groupWords(
      roomName,
      keys.map((k) => facts(employeeOf.get(k)!)),
      id === MORE ? moreProjects.length : null,
      received !== null && keys.includes(received),
    );
    targets.push({ id: `group:${id}`, group: room, kind: "group", rect, label: words.label, tag: words.tag, context: null, keys });
  };
  const sharedGroup = (zone: SharedZone) => {
    for (const s of sharedSpots(zone)) figure(s, zone);
    group(zone, zone, zoneGroupTarget(columns, zone), plaqueText(zone));
  };
  sharedGroup("reception");
  sharedGroup("decision");
  {
    const words = orchestratorWords(presence, proposals);
    const kind: TargetKind = presence ? "orchestrator" : "chair";
    targets.push({ id: kind, group: "decision", kind, rect: figureTarget(chairAt), label: words.label, tag: words.tag, context: null, keys: [] });
  }
  sharedGroup("alert");
  regular.forEach((project, slot) => {
    const { desks: d, chips: c } = spotsOf[slot]!;
    const frontToBack = [...d].sort((a, b) => b.gx + b.gy - (a.gx + a.gy) || b.gx - a.gx);
    for (const s of [...frontToBack, ...c]) figure(s, officeId(slot));
    group(officeId(slot), officeId(slot), chipTarget(columns, cells[slot]!), project);
  });
  sharedGroup("lounge");
  if (overflow) group(MORE, MORE, moreOfficesTarget(columns, cells[MAX_OFFICES - 1]!), plaqueText(MORE));

  // A sub-agent's target opens its lead's file, where it does not overlap
  // another target (story 4.6's property): at the prototype's offsets its
  // lead's own target covers it.
  for (const h of helpers) {
    const rect = figureTarget(h.at);
    if (targets.some((t) => intersects(t.rect, rect))) continue;
    const lead = cardOf.get(h.lead)!;
    const words = helperWords(lead, lead.subagents.find((a) => helperKey(h.lead, a.agent_id) === h.key)!);
    targets.push({ id: `helper:${h.key}`, group: officeId(slotOf.get(h.project)!), kind: "helper", rect, label: words.label, tag: words.tag, context: words.context, keys: [h.lead] });
  }

  const groups: Group[] = targets
    .filter((t) => t.kind === "group")
    .map((t) => ({ id: t.id.slice("group:".length), room: t.group, keys: t.keys }));

  const nextLast: Record<string, LastPlace> = {};
  for (const e of employees) nextLast[e.key] = { spot: e.spot, group: e.group, pose: e.pose, facing: e.facing };
  const nextDesks: Record<string, (string | null)[]> = {};
  for (const [project, list] of desks) if (list.some((k) => k !== null)) nextDesks[project] = [...list];

  return {
    columns,
    canvas: canvas(columns),
    rooms,
    employees,
    helpers,
    desks: deskList,
    groups,
    targets,
    orchestrator,
    display,
    summary: summaryText(board, employees),
    memory: {
      floor: board.floor,
      offices: order,
      desks: nextDesks,
      inRun: true,
      chips,
      sticky,
      last: nextLast,
      briefed,
    },
  };
}

/** The alert room's state (AD-175): lit while someone stands in it. */
export function alertLit(layout: SceneLayout): boolean {
  return layout.rooms.some((room) => room.kind === "alert" && room.plaque.state === "alert");
}

export type Direction = "up" | "down" | "left" | "right";

const AXIS: Record<Direction, Point> = {
  up: { x: 0, y: -1 },
  down: { x: 0, y: 1 },
  left: { x: -1, y: 0 },
  right: { x: 1, y: 0 },
};

/** The target an arrow moves to (D-187, AD-63): within the focused
 *  target's group, a centre within 45 degrees of the arrow's axis (the
 *  perpendicular offset no larger than the forward one, which is above 0),
 *  the nearest by distance, ties by tab order; `null` when none. */
export function nearestTarget(targets: readonly Target[], fromId: string, direction: Direction): string | null {
  const from = targets.find((t) => t.id === fromId);
  if (!from) return null;
  const origin = centreOf(from.rect);
  const axis = AXIS[direction];
  let best: { id: string; distance: number } | null = null;
  for (const t of targets) {
    if (t === from || t.group !== from.group) continue;
    const c = centreOf(t.rect);
    const dx = c.x - origin.x;
    const dy = c.y - origin.y;
    const forward = dx * axis.x + dy * axis.y;
    const across = Math.abs(dx * axis.y - dy * axis.x);
    if (forward <= 0 || across > forward) continue;
    const distance = Math.hypot(dx, dy);
    if (!best || distance < best.distance) best = { id: t.id, distance };
  }
  return best?.id ?? null;
}
