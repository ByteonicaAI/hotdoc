import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";
import { log } from "./lib/logger";

window.addEventListener("error", (e) => {
  log.error("window error", {
    message: e.message,
    filename: e.filename,
    lineno: e.lineno,
    colno: e.colno,
    stack: e.error instanceof Error ? e.error.stack : undefined,
  });
});

window.addEventListener("unhandledrejection", (e) => {
  const reason = e.reason instanceof Error ? e.reason.message : String(e.reason);
  log.error("unhandled rejection", { reason });
});

const app = mount(App, {
  target: document.getElementById("app")!,
});

export default app;
