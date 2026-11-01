// One name per session (story 4.20, the owner's item 5, D-1044). A session
// is named by its own name, `Card.name`: the name Claude Code gives it, or
// the folder's when it has none. That name is the only one a session
// carries on any surface (the scene's name tag, the session file's title,
// the board card, the register, Focus). Its project is context, never a
// second name: marked by its colour dot, smaller than the name, and on a
// line of text written after the name, set apart. A project's name is
// never followed by a bare number either, so a room's plaque counts its
// people behind a glyph.
import type { Card } from "./generated/Card";

/** The project a session belongs to, as context beside its name; `null`
 *  when the name already says it, as a session without a name of its own
 *  takes its folder's, which is usually its project's. */
export function projectContext(card: Pick<Card, "name" | "project">): string | null {
  return card.project.name.toLowerCase() === card.name.toLowerCase() ? null : card.project.name;
}
