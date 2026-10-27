import type { ProjectColor } from "./generated/ProjectColor";

// Exhaustive on purpose: a new Rust `ProjectColor` variant fails the type
// check here until it has a colour (AD-13).
export const PROJECT_COLORS: Record<ProjectColor, string> = {
  green: "var(--color-project-green)",
  blue: "var(--color-project-blue)",
  terracotta: "var(--color-project-terracotta)",
};

export function projectColor(color: ProjectColor | null): string {
  return color ? PROJECT_COLORS[color] : "var(--color-project-none)";
}
