// The window's transitions (story 4.15, the owner's item 8, D-545): short
// fades and slides on opacity and transform only, for what appears and
// disappears. Svelte runs them through the Web Animations API, which the
// global reduced-motion rule of app.css does not reach, so each one asks
// the system at the moment it runs and takes no time under reduced motion.
// Nothing here loops: a transition runs once, when its element comes or
// goes (FR-9).
import { fade, fly, type FadeParams, type FlyParams, type TransitionConfig } from "svelte/transition";

/** How long a fade or a short slide lasts, in ms. */
export const MOTION_MS = 160;

/** True when the system asks for reduced motion, read at each use. */
export function reducedMotion(): boolean {
  return globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
}

const duration = () => (reducedMotion() ? 0 : MOTION_MS);

/** A fade in or out. */
export function fadeMotion(node: Element, params: FadeParams = {}): TransitionConfig {
  return fade(node, { duration: duration(), ...params });
}

/** A short slide with a fade, `y` px from below (negative: from above). */
export function slideMotion(node: Element, params: FlyParams = {}): TransitionConfig {
  return fly(node, { duration: duration(), y: 6, ...params });
}
