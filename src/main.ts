import { mount } from "svelte";
import "./app.css";
import Root from "./Root.svelte";
import { api } from "./lib/api";
import { layouts } from "./lib/layout.svelte";
import { theme } from "./lib/theme.svelte";
import { palette } from "./lib/palette.svelte";

// Before the first mount, so the window never paints one paper and then swaps to the other.
// The palette follows the paper, so it is wired in before the first `apply()` and started
// after it: each paper keeps its own colours, and switching swaps the whole set.
theme.onPaperChange = () => palette.load();
theme.start();
palette.start();
layouts.start();

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
