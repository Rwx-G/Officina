// The dark theme and its switching (story 4.13; FR-8, AD-67, D-113): the
// system's scheme on Auto, a manual Light or Dark that wins and is kept, the
// theme set before the app mounts, and the header logo following it. Token
// values are read from the page's own style sheet, never retyped.
import { expect, test, type Page } from "@playwright/test";
import { loadBoard, openBoard, stylesheetApplied } from "./fixtures";
import { VIEW_STORAGE_KEY, defaultViewState, serialiseViewState } from "../src/lib/view.svelte";
import type { Theme } from "../src/lib/theme";

const themeSwitch = (page: Page) => page.getByTestId("theme-switch");

/** The colour tokens (the window's, the scene's, the figures') each theme's
 *  rule declares, and the value each one has on the root now. The build's
 *  minifier adds properties of its own beside them, which are left out. */
function tokens(page: Page): Promise<{ light: Record<string, string>; dark: Record<string, string>; now: Record<string, string> }> {
  return page.evaluate(() => {
    const read = (selector: string): Record<string, string> => {
      const out: Record<string, string> = {};
      for (const sheet of Array.from(document.styleSheets)) {
        for (const rule of Array.from(sheet.cssRules)) {
          if (!(rule instanceof CSSStyleRule) || rule.selectorText !== selector) continue;
          for (const name of Array.from(rule.style)) {
            if (/^--(color|scene|figure)-/.test(name)) out[name] = rule.style.getPropertyValue(name).trim();
          }
        }
      }
      return out;
    };
    const light = read(":root");
    const dark = read(':root[data-theme="dark"]');
    const style = getComputedStyle(document.documentElement);
    const now = Object.fromEntries(Object.keys(dark).map((name) => [name, style.getPropertyValue(name).trim()]));
    return { light, dark, now };
  });
}

/** The root holds `theme`'s values for every token the dark rule sets, and
 *  the two themes differ on the page's background. */
async function expectTheme(page: Page, theme: Theme): Promise<void> {
  const { light, dark, now } = await tokens(page);
  expect(Object.keys(dark).length).toBeGreaterThan(10);
  expect(dark["--color-bg"]).not.toBe(light["--color-bg"]);
  const expected = Object.fromEntries(Object.keys(dark).map((name) => [name, (theme === "dark" ? dark : light)[name]]));
  expect(now).toEqual(expected);
  await expect(themeSwitch(page)).toHaveAttribute("data-theme-resolved", theme);
}

async function cycleTo(page: Page, choice: string): Promise<void> {
  for (let i = 0; i < 3 && (await themeSwitch(page).getAttribute("data-theme-choice")) !== choice; i++) {
    await themeSwitch(page).click();
  }
  await expect(themeSwitch(page)).toHaveAttribute("data-theme-choice", choice);
}

test.describe("themes", () => {
  test("the system dark scheme gives the dark tokens", async ({ page }) => {
    await page.emulateMedia({ colorScheme: "dark" });
    await openBoard(page, loadBoard("busy"));
    await expect(themeSwitch(page)).toHaveAttribute("data-theme-choice", "auto");
    await expect(themeSwitch(page)).toHaveAccessibleName("Theme: Auto (dark)");
    expect(await page.evaluate(() => document.documentElement.hasAttribute("data-theme"))).toBe(false);
    await expectTheme(page, "dark");
    // The page itself, not only the tokens: the body is painted with them.
    const { dark } = await tokens(page);
    const painted = await page.evaluate((bg) => {
      const probe = document.createElement("div");
      probe.style.color = bg;
      document.body.append(probe);
      const wanted = getComputedStyle(probe).color;
      probe.remove();
      return { wanted, body: getComputedStyle(document.body).backgroundColor };
    }, dark["--color-bg"]!);
    expect(painted.body).toBe(painted.wanted);
    // Auto follows a system switch while the window is open.
    await page.emulateMedia({ colorScheme: "light" });
    await expectTheme(page, "light");
    await expect(themeSwitch(page)).toHaveAccessibleName("Theme: Auto (light)");
  });

  for (const [system, manual] of [
    ["dark", "light"],
    ["light", "dark"],
  ] as const) {
    test(`a manual Light or Dark wins over the system and survives a reload (${manual} over ${system})`, async ({ page }) => {
      await page.emulateMedia({ colorScheme: system });
      await openBoard(page, loadBoard("busy"));
      // From the keyboard: the control is a button in the tab order.
      await themeSwitch(page).focus();
      await page.keyboard.press("Enter");
      await cycleTo(page, manual);
      await expect(themeSwitch(page)).toBeFocused();
      await expect(themeSwitch(page)).toHaveAccessibleName(`Theme: ${manual === "dark" ? "Dark" : "Light"}`);
      expect(await page.evaluate(() => document.documentElement.getAttribute("data-theme"))).toBe(manual);
      await expectTheme(page, manual);
      await page.reload();
      await stylesheetApplied(page);
      await page.getByTestId("column-needs_you").waitFor();
      await expect(themeSwitch(page)).toHaveAttribute("data-theme-choice", manual);
      await expectTheme(page, manual);
      // The system switching back changes nothing while a choice is made.
      await page.emulateMedia({ colorScheme: manual === "dark" ? "light" : "dark" });
      await expectTheme(page, manual);
      // Back to Auto: the system decides again.
      await cycleTo(page, "auto");
      expect(await page.evaluate(() => document.documentElement.hasAttribute("data-theme"))).toBe(false);
      await expectTheme(page, manual === "dark" ? "light" : "dark");
    });
  }

  test("the theme is set before the app mounts", async ({ page }) => {
    await page.emulateMedia({ colorScheme: "light" });
    await page.addInitScript(
      ([key, value]) => {
        localStorage.setItem(key, value);
        // The root's theme and the body's colour when the app first puts
        // something in its mount point.
        new MutationObserver((_, observer) => {
          const app = document.getElementById("app");
          if (!app?.firstChild) return;
          observer.disconnect();
          (window as unknown as { __atMount: unknown }).__atMount = {
            theme: document.documentElement.getAttribute("data-theme"),
            bg: getComputedStyle(document.documentElement).getPropertyValue("--color-bg").trim(),
          };
        }).observe(document, { childList: true, subtree: true });
      },
      [VIEW_STORAGE_KEY, serialiseViewState({ ...defaultViewState(), theme: "dark" })] as const,
    );
    await openBoard(page, loadBoard("busy"));
    const { dark } = await tokens(page);
    const atMount = await page.evaluate(() => (window as unknown as { __atMount: unknown }).__atMount);
    expect(atMount).toEqual({ theme: "dark", bg: dark["--color-bg"] });
  });

  test("the header logo follows the theme", async ({ page }) => {
    const logo = page.getByTestId("header-logo");
    const isDarkLogo = async () => /logo-dark/.test((await logo.getAttribute("src")) ?? "");
    await page.emulateMedia({ colorScheme: "light" });
    await openBoard(page, loadBoard("busy"));
    expect(await isDarkLogo()).toBe(false);
    // The system's scheme on Auto, live.
    await page.emulateMedia({ colorScheme: "dark" });
    await expect.poll(isDarkLogo).toBe(true);
    // A manual choice over it.
    await cycleTo(page, "light");
    await expect.poll(isDarkLogo).toBe(false);
    await cycleTo(page, "dark");
    await expect.poll(isDarkLogo).toBe(true);
    // The image loads under the window's policy; a switched source loads
    // after the attribute changes, so the check waits for it.
    await expect.poll(() => logo.evaluate((img: HTMLImageElement) => img.complete && img.naturalWidth > 0)).toBe(true);
  });
});
