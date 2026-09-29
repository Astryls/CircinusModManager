// Which paper the window is on.
//
// Three sets of tokens, none invented here: "dark" is circinus.sh's own `:root`, "light" an
// e-ink step of its light set, and "oled" the dark set with its ground taken to #000. The choice
// is a property of this machine and this screen, not of the mod list, so it lives in
// localStorage and never goes near ModsConfig or the user data the backend writes -- moving a
// list between machines must not drag a theme with it.
//
// **Two axes, not three choices.** `paper` is light or dark and is what the toggle flips;
// `darkVariant` decides which dark you get and is a setting. Modelling OLED as a third value of
// one enum would have made the toggle a three-state cycle, so somebody who wanted to glance at
// the light paper and come back would land somewhere they did not start. The setting also reads
// the way the feature was asked for: use the black one instead of the normal dark one.
//
// `data-theme` is written on <html> rather than on a wrapper because the scrollbar, the form
// controls and the window's own background all read `color-scheme` off the root element. Setting
// it a level down leaves a light page with dark scrollbars.

export type Paper = "dark" | "light";
export type DarkVariant = "normal" | "oled";

const KEY = "circinus.paper";
const DARK_KEY = "circinus.dark";

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

function storedDark(): DarkVariant {
  try {
    return localStorage.getItem(DARK_KEY) === "oled" ? "oled" : "normal";
  } catch {
    return "normal";
  }
}

class Theme {
  /** Never null: there is always a paper, even before a choice is made. */
  paper = $state<Paper>("dark");
  /** Which dark. Ignored entirely while the paper is light. */
  darkVariant = $state<DarkVariant>("normal");
  /** True until the user picks one, so the window can keep following the machine. */
  auto = $state(true);

  /** Call once, as early as possible -- before first paint if you can, or the window flashes. */
  start() {
    const s = stored();
    this.auto = s === null;
    this.paper = s ?? preferred();
    this.darkVariant = storedDark();
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

  /** The value of `data-theme`, or null for the plain dark set that lives on bare `:root`. */
  get attr(): string | null {
    if (this.paper === "light") return "light";
    return this.darkVariant === "oled" ? "oled" : null;
  }

  private apply() {
    const el = document.documentElement;
    const a = this.attr;
    if (a) el.setAttribute("data-theme", a);
    else el.removeAttribute("data-theme");
  }

  private remember(key: string, value: string) {
    try {
      localStorage.setItem(key, value);
    } catch {
      // Not being able to remember it is not a reason to refuse to change it.
    }
  }

  set(p: Paper) {
    this.paper = p;
    this.auto = false;
    this.apply();
    this.remember(KEY, p);
  }

  /** Choose which dark. Applies immediately even while the light paper is showing, so the
   *  setting is already right the next time the toggle comes back to dark. */
  setDark(v: DarkVariant) {
    this.darkVariant = v;
    this.apply();
    this.remember(DARK_KEY, v);
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
