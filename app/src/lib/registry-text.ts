// What the window says about the registry's state (AD-54, D-47). The state
// comes from Rust and is mapped through an exhaustive `Record`, so a state
// added there fails `pnpm check` until it has a text here.
import type { RegistryState } from "./generated/RegistryState";

/** The Board view's banner per state; `null` where nothing needs saying.
 *  The window creates the registry before its first board, so only a board
 *  printed by `officina board --json` can say `absent`. */
export const REGISTRY_STATE_TEXT: Record<RegistryState, string | null> = {
  ready: null,
  absent: null,
  older: "The registry was written by an older Officina: the window migrates it when it starts.",
  newer:
    "The registry was written by a newer Officina: it is read only, so nothing is recorded and no event file is deleted.",
  unavailable:
    "The registry is unavailable: nothing is recorded and no event file is deleted until the window starts again.",
  network:
    "The registry is on a network folder, where it cannot work: nothing is recorded and no event file is deleted. Set OFFICINA_HOME to a local folder.",
};
