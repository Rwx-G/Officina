<script lang="ts">
  // The window's theme (AD-67): one button cycling Auto, Light and Dark,
  // showing the choice and, on Auto, the theme the system gives. A fixed
  // header control: every label it can show sits in its box, so a click or
  // a system switch moves nothing beside it (D-517). The labels are short,
  // since Scene mode's one-line header shares its width with the status
  // chips (D-526); the accessible name says what they are the choice of.
  import { nextTheme, type Theme, type ThemeChoice } from "../lib/theme";
  import StableLabel from "./StableLabel.svelte";

  interface Props {
    theme: ThemeChoice;
    /** The theme the window shows. */
    resolved: Theme;
    onCycle: () => void;
  }
  let { theme, resolved, onCycle }: Props = $props();

  const NAMES: Record<ThemeChoice, string> = { auto: "Auto", light: "Light", dark: "Dark" };

  type LabelKey = `auto-${Theme}` | Theme;
  const LABELS: Record<LabelKey, string> = {
    "auto-light": "Auto (light)",
    "auto-dark": "Auto (dark)",
    light: "Light",
    dark: "Dark",
  };

  let current = $derived<LabelKey>(theme === "auto" ? `auto-${resolved}` : theme);
  let title = $derived(
    `${theme === "auto" ? "Follows the system's theme" : `${NAMES[theme]} whatever the system's theme`}; click for ${NAMES[nextTheme(theme)]}`,
  );
</script>

<button
  class="theme"
  data-testid="theme-switch"
  data-theme-choice={theme}
  data-theme-resolved={resolved}
  aria-label="Theme: {LABELS[current]}"
  {title}
  onclick={onCycle}
>
  <StableLabel labels={LABELS} {current} />
</button>

<style>
  .theme {
    min-height: var(--target-min);
    padding: 0 12px;
    border-radius: var(--radius-control);
    border: 1px solid var(--color-text);
    background: var(--color-surface);
    font-weight: 600;
    cursor: pointer;
  }
</style>
