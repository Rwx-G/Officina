import { describe, expect, it } from "vitest";
import type { ProjectColor } from "../generated/ProjectColor";
import { PROJECT_COLORS } from "../colors";
import type { Pose } from "./layout";
import type { Facing } from "./places";
import { HAIR_STYLES, LOOKS, badgeMarkup, createRig, lookOf, marksMarkup, setPose, ticketMarkup, ticketOn, type Rig } from "./rig";

const POSES: Pose[] = ["standing", "seated-facing", "seated-working"];
const FACINGS: Facing[] = ["front", "back"];
const COLOURS: (ProjectColor | null)[] = [...(Object.keys(PROJECT_COLORS) as ProjectColor[]), null];

function rig(colour: ProjectColor | null = "green", cap = true): Rig {
  const r = createRig(document, { look: lookOf("pid:1:1"), colour, cap, scale: 1 });
  document.createElementNS("http://www.w3.org/2000/svg", "svg").append(r.node);
  return r;
}

/** The text nodes under `node` that hold anything but blanks. */
function textNodes(node: Node): string[] {
  const out: string[] = [];
  const walker = document.createTreeWalker(node, NodeFilter.SHOW_TEXT);
  for (let n = walker.nextNode(); n; n = walker.nextNode()) if (n.textContent?.trim()) out.push(n.textContent);
  return out;
}

const visible = (el: Element) => el.getAttribute("display") !== "none";

describe("rig", () => {
  it("a rig has three pose groups and two views", () => {
    const r = rig();
    const groups = Object.values(r.poses);
    expect(new Set(groups).size).toBe(POSES.length);
    for (const group of groups) expect(r.node.contains(group)).toBe(true);
    expect(r.fronts.length).toBeGreaterThan(0);
    expect(r.backs.length).toBe(r.fronts.length);
    for (const el of [...r.fronts, ...r.backs]) expect(r.poses.standing.contains(el)).toBe(true);
    // Built with the standing pose shown, from the front.
    expect(groups.filter(visible)).toEqual([r.poses.standing]);
    expect(r.fronts.every(visible)).toBe(true);
    expect(r.backs.some(visible)).toBe(false);
  });

  it("a pose change toggles visibility and rebuilds nothing", () => {
    const r = rig();
    const nodes = [...r.node.querySelectorAll("*")];
    const observer = new MutationObserver(() => undefined);
    observer.observe(r.node, { subtree: true, childList: true, attributes: true, characterData: true });
    for (const pose of POSES) {
      for (const facing of FACINGS) {
        for (const hand of [false, true]) {
          setPose(r, pose, facing, hand);
          const records = observer.takeRecords();
          for (const record of records) {
            expect(record.type).toBe("attributes");
            expect(["display", "transform"]).toContain(record.attributeName);
          }
          expect(Object.values(r.poses).filter(visible)).toEqual([r.poses[pose]]);
          if (pose === "standing") {
            expect(r.fronts.every(visible)).toBe(facing === "front");
            expect(r.backs.every(visible)).toBe(facing === "back");
          }
          // The same pose again writes nothing.
          setPose(r, pose, facing, hand);
          expect(observer.takeRecords()).toEqual([]);
        }
      }
    }
    observer.disconnect();
    expect([...r.node.querySelectorAll("*")]).toEqual(nodes);
  });

  it("markup holds no text node for every look, pose and colour", () => {
    for (const [skin, hair] of LOOKS) {
      for (const style of HAIR_STYLES) {
        for (const colour of COLOURS) {
          for (const cap of [true, false]) {
            const r = createRig(document, { look: { skin, hair, style }, colour, cap, scale: 1 });
            for (const pose of POSES) {
              setPose(r, pose, "front", false);
              expect(textNodes(r.node), `${skin}/${hair}/${style}/${colour}/${cap}/${pose}`).toEqual([]);
            }
          }
        }
      }
    }
  });

  it("a look comes from the card key, among the allowed pairs only", () => {
    const keys = Array.from({ length: 200 }, (_, i) => `pid:${i}:${i * 7}`);
    for (const key of keys) {
      const look = lookOf(key);
      expect(lookOf(key)).toEqual(look);
      expect(LOOKS.some(([skin, hair]) => skin === look.skin && hair === look.hair)).toBe(true);
      expect(HAIR_STYLES).toContain(look.style);
    }
    // The keys spread over the looks: a look is not one for all.
    expect(new Set(keys.map((key) => JSON.stringify(lookOf(key)))).size).toBeGreaterThan(LOOKS.length);
  });

  it("marks carry numbers only, as digits", () => {
    expect(() => ticketMarkup(1.5, false)).toThrow();
    expect(() => badgeMarkup(Number.NaN, "green")).toThrow();
    const holder = document.createElementNS("http://www.w3.org/2000/svg", "g");
    holder.innerHTML = ticketMarkup(7, false) + badgeMarkup(3, null) + ticketMarkup(null, false);
    expect(textNodes(holder)).toEqual(["7", "3"]);
    // No chest mark on a back turned to the author.
    const marks = { ticket: 2, approval: false, deskBadge: null, hand: false, ring: false, question: false };
    expect(ticketOn(marks, "standing", "back", false)).toBe("");
    expect(ticketOn(marks, "seated-working", "back", false)).toBe("");
    expect(ticketOn(marks, "standing", "front", false)).toContain(">2<");
    // The ticket is the badge layer's, never among the figure's own marks.
    expect(marksMarkup(marks, "standing", "front", "green")).toBe("");
  });
});
