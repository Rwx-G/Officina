import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";
import { applyTheme } from "./lib/theme";
import { storedViewState } from "./lib/view.svelte";

// The stored theme before anything is drawn, so a dark choice never shows a
// light window first (AD-67).
applyTheme(storedViewState().theme);

if (import.meta.env.MODE === "e2e") {
  const { installE2eMock } = await import("./e2e-mock");
  installE2eMock();
}

const target = document.getElementById("app");
if (!target) throw new Error("missing #app mount point");
export default mount(App, { target });
