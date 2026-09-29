// Which paper the window is on.
//
// Two of them, both taken from circinus.sh rather than invented here: "dark" is the site's own
// `:root`, "light" an e-ink step of its light set. The choice is a property of this machine and
// this screen, not of the mod list, so it lives in localStorage and never goes near ModsConfig
// or the user data the backend writes -- moving a list between machines must not drag a theme
// with it.
//
// `data-theme` is written on <html> rather than on a wrapper because the scrollbar, the form
// controls and the window's own background all read `color-scheme` off the root element. Setting
// it a level down leaves a light page with dark scrollbars.

export type Paper = "dark" | "light";

const KEY = "circinus.paper";

/** What the machine prefers, for the very first run before anybody has chosen. */
function preferred(): Paper {
  try {
    return window.matchMedia?.("(prefers-color-scheme: light)").matches ? "light" : "dark";
  } catch {
    return "dark";
  }
}

function stored(): Paper | null {
  try {
    const v = localStorage.getItem(KEY);
    return v === "dark" || v === "light" ? v : null;
  } catch {
    // A locked-down webview can throw on the accessor itself, not just return null.
    return null;
  }
}

class Theme {
  /** Never null: there is always a paper, even before a choice is made. */
  paper = $state<Paper>("dark");
  /** True until the user picks one, so the window can keep following the machine. */
  auto = $state(true);

  /** Call once, as early as possible -- before first paint if you can, or the window flashes. */
  start() {
    const s = stored();
    this.auto = s === null;
    this.paper = s ?? preferred();
    this.apply();
    try {
      window.matchMedia?.("(prefers-color-scheme: light)").addEventListener("change", (e) => {
        if (!this.auto) return;
        this.paper = e.matches ? "light" : "dark";
        this.apply();
      });
    } catch {
      // No matchMedia is fine; the window just stops following the machine.
    }
  }

  private apply() {
    const el = document.documentElement;
    if (this.paper === "light") el.setAttribute("data-theme", "light");
    else el.removeAttribute("data-theme");
  }

  set(p: Paper) {
    this.paper = p;
    this.auto = false;
    this.apply();
    try {
      localStorage.setItem(KEY, p);
    } catch {
      // Not being able to remember it is not a reason to refuse to change it.
    }
  }

  toggle() {
    this.set(this.paper === "dark" ? "light" : "dark");
  }

  /** What the toggle should say it will do, for the tooltip and the accessible name. */
  get nextLabel() {
    return this.paper === "dark" ? "Switch to the light paper" : "Switch to the dark paper";
  }
}

export const theme = new Theme();
