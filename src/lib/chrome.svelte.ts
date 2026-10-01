// The window's own frame, when the operating system's one does not match the window.
//
// Circinus has no title bar by design -- the directory column carries identity and the
// actions, and the one row above the content is the search line. On Windows that leaves an
// OS-drawn caption bar in a completely different grey sitting on top of a window that has
// otherwise been drawn to one palette, which is what this is for: the three buttons move into
// the search line, in the window's own tokens.
//
// **It is switched at runtime, never baked into the build**, and that is the important part.
// Setting `decorations: false` in `tauri.conf.json` would make this a property of the binary:
// if the custom frame misbehaved on somebody's machine -- a window manager that does not
// support the drag loop, a display scale that puts the buttons somewhere unreachable -- there
// would be no way back without a new release. `setDecorations(true)` puts the real caption bar
// back instantly, so the switch in Settings is a genuine escape hatch rather than a preference.
//
// Windows only, on purpose. macOS already has the traffic lights in a place the layout can
// make room for and a user expects them there; the Linux desktops each have their own caption
// and their own idea of where the buttons go, and replacing that is a worse experience than
// matching it. The toggle is simply absent on both.
//
// What is lost with the system frame, and what is not:
//   - Dragging and Aero Snap are kept. `startDragging` hands the window to the OS drag loop
//     (`WM_NCLBUTTONDOWN` with `HTCAPTION`), so edge snapping and Win+Arrow behave normally.
//   - Resizing is kept, through `startResizeDragging` on eight invisible grips. The OS
//     resize borders are gone with the frame, so without those the window cannot be resized
//     at all -- which is why they are not optional.
//   - **Snap Layouts are lost**: the flyout when you hover the maximize button needs the
//     window to answer `WM_NCHITTEST` with `HTMAXBUTTON`, and that cannot be done from the
//     webview. Anybody who relies on it should leave this off, and the setting says so.

import { api, inTauri } from "$lib/api";

const KEY = "circinus.frame";

/** Windows only. `navigator.userAgentData` is the reliable one in a modern webview; the
 *  platform string is the fallback for the WebView2 versions that do not have it. */
function onWindows(): boolean {
  try {
    const uaData = (navigator as unknown as { userAgentData?: { platform?: string } }).userAgentData;
    if (uaData?.platform) return uaData.platform === "Windows";
    return /Win/i.test(navigator.platform || navigator.userAgent);
  } catch {
    return false;
  }
}

async function win() {
  const mod = await import("@tauri-apps/api/window");
  return mod.getCurrentWindow();
}

class Chrome {
  /** Is the in-window frame available at all? False everywhere but Tauri on Windows. */
  available = $state(false);
  /** Is it on? Meaningless when `available` is false. */
  on = $state(false);
  /** Mirrors the real window, so the middle button can draw the right glyph. */
  maximized = $state(false);

  async start() {
    this.available = inTauri && onWindows();
    // `?frame` draws the buttons in the browser mock. There is no window behind them there, so
    // every call below is a no-op that its own catch swallows -- but the geometry is real, and
    // geometry is the half of this that can be checked without Windows in front of you. The
    // layout is the part that goes wrong quietly; whether `startDragging` snaps is not
    // something a browser can answer either way.
    try {
      if (!inTauri && new URLSearchParams(location.search).has("frame")) {
        this.available = true;
        this.on = true;
        return;
      }
    } catch {
      /* no location: not a browser, carry on */
    }
    if (!this.available) return;
    let want = true;
    try {
      const v = localStorage.getItem(KEY);
      if (v === "system") want = false;
    } catch {
      // A locked-down webview throws on the accessor. The default stands.
    }
    await this.set(want);
    try {
      const w = await win();
      this.maximized = await w.isMaximized();
      // The window can be maximized by the OS -- a double-click, Win+Up, a snap -- so the
      // button follows the window rather than the button's own last press.
      await this.listen();
      await w.onResized(async () => {
        try {
          this.maximized = await w.isMaximized();
        } catch {
          /* the window is going away */
        }
      });
    } catch {
      /* no window API: the frame still draws, it just cannot track the state */
    }
  }

  /** True while the pointer is over the maximise button, as reported by the native hit test.
   *
   *  Once `WM_NCHITTEST` answers `HTMAXBUTTON` the shell treats that rectangle as part of the
   *  caption, and the webview stops getting ordinary mouse events for it -- so `:hover` never
   *  fires and the button would sit dead under the pointer while the Snap Layouts flyout is
   *  open above it. Rust forwards the non-client move and leave, and this is what they set. */
  snapHover = $state(false);

  /** Turn the in-window frame on or off. Takes effect at once, both ways. */
  async set(on: boolean) {
    if (!this.available) return;
    this.on = on;
    try {
      const w = await win();
      await w.setDecorations(!on);
      // The hit test goes on and off with the frame. While the system caption bar is showing
      // it owns the whole non-client area, and a hit test still claiming part of it would be
      // fighting the shell for the real maximise button.
      await api.setWindowFrame(on);
      if (!on) await api.setMaximiseRect(0, 0, 0, 0);
    } catch (e) {
      // If the window refuses, the system frame is what is still showing -- so say that
      // rather than leaving a switch claiming something that did not happen.
      console.warn("could not change the window frame", e);
      this.on = !on;
      return;
    }
    try {
      localStorage.setItem(KEY, on ? "custom" : "system");
    } catch {
      /* not remembering it is not a reason to refuse it */
    }
  }

  async minimize() {
    try {
      (await win()).minimize();
    } catch {
      /* nothing sensible to do: the button simply does not work */
    }
  }

  async toggleMaximize() {
    try {
      const w = await win();
      await w.toggleMaximize();
      this.maximized = await w.isMaximized();
    } catch {
      /* as above */
    }
  }

  async close() {
    try {
      (await win()).close();
    } catch {
      /* as above */
    }
  }

  /** Tell Rust where the maximise button is, in physical pixels relative to the client area.
   *
   *  Called whenever it moves: the window resizes, the display scale changes, the frame is
   *  switched. `devicePixelRatio` is the conversion, because the rectangle Win32 compares
   *  against is in device pixels and `getBoundingClientRect` is in CSS ones -- at 150% scale,
   *  which is an ordinary Windows setting, forgetting it puts the hit area two thirds of the
   *  way to where the button is. */
  reportMaximiseRect(el: HTMLElement | null) {
    if (!this.available || !this.on || !el) return;
    const r = el.getBoundingClientRect();
    const s = window.devicePixelRatio || 1;
    void api.setMaximiseRect(Math.round(r.x * s), Math.round(r.y * s), Math.round(r.width * s), Math.round(r.height * s));
  }

  /** Listen for what the native hit test takes over: the hover the webview can no longer see,
   *  and the click it swallowed. */
  async listen() {
    if (!this.available) return;
    try {
      const ev = await import("@tauri-apps/api/event");
      await ev.listen("snap:hover", () => (this.snapHover = true));
      await ev.listen("snap:leave", () => (this.snapHover = false));
      await ev.listen("snap:toggle", () => this.toggleMaximize());
    } catch {
      /* no event API: the button still works, it just will not light up on hover */
    }
  }

  /** Start an OS resize from one of the eight grips. The system's resize borders go with the
   *  frame, so without this the window cannot be resized at all. */
  async resizeFrom(dir: string) {
    try {
      const mod = await import("@tauri-apps/api/window");
      await mod.getCurrentWindow().startResizeDragging(dir as never);
    } catch {
      /* as above */
    }
  }
}

export const chrome = new Chrome();
