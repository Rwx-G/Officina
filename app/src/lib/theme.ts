// The window's theme (AD-67): Auto follows the system's colour scheme, Light
// and Dark override it. app.css applies the dark tokens under
// `data-theme="dark"`, and under the system's dark scheme unless
// `data-theme="light"`; Auto therefore leaves the attribute off.

/** The theme control's choices, in the order it cycles through them. */
export const THEME_CHOICES = ["auto", "light", "dark"] as const;
export type ThemeChoice = (typeof THEME_CHOICES)[number];

/** The theme the window shows. */
export type Theme = Exclude<ThemeChoice, "auto">;

export const DEFAULT_THEME: ThemeChoice = "auto";

/** The media query app.css's Auto rule follows. */
export const DARK_SCHEME_QUERY = "(prefers-color-scheme: dark)";

export function isThemeChoice(value: unknown): value is ThemeChoice {
  return THEME_CHOICES.includes(value as ThemeChoice);
}

/** The choice after `choice` in the cycle. */
export function nextTheme(choice: ThemeChoice): ThemeChoice {
  return THEME_CHOICES[(THEME_CHOICES.indexOf(choice) + 1) % THEME_CHOICES.length]!;
}

export function resolveTheme(choice: ThemeChoice, systemDark: boolean): Theme {
  if (choice !== "auto") return choice;
  return systemDark ? "dark" : "light";
}

/** Whether the system asks for a dark scheme now. */
export function systemPrefersDark(): boolean {
  return globalThis.matchMedia?.(DARK_SCHEME_QUERY).matches ?? false;
}

/** Sets the root's `data-theme` for `choice`: none on Auto, so the media
 *  query decides. */
export function applyTheme(choice: ThemeChoice, root: HTMLElement = document.documentElement): void {
  if (choice === "auto") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", choice);
}
