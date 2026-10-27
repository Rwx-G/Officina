import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";

if (import.meta.env.MODE === "e2e") {
  const { installE2eMock } = await import("./e2e-mock");
  installE2eMock();
}

const target = document.getElementById("app");
if (!target) throw new Error("missing #app mount point");
export default mount(App, { target });
