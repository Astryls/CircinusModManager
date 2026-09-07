import { mount } from "svelte";
import "./app.css";
import Root from "./Root.svelte";
import { api } from "./lib/api";

// Anything that escapes goes into the same log as the rest of the app. A message that only ever
// reaches a console nobody opens is a message nobody has: the window went black for a user and
// there was nothing on disk to read afterwards.
function record(what: string, err: unknown) {
  const e = err as { message?: string; stack?: string } | undefined;
  const message = `${what}: ${e?.message ?? String(err)}`;
  console.error(`[circinus] ${message}`, err);
  api.logFromTheWindow(message, e?.stack).catch(() => {
    /* the backend is the thing that just failed, possibly; the console line above stands */
  });
}
window.addEventListener("error", (e) => record("uncaught", e.error ?? e.message));
window.addEventListener("unhandledrejection", (e) => record("unhandled rejection", e.reason));

const app = mount(Root, { target: document.getElementById("app")! });

export default app;
