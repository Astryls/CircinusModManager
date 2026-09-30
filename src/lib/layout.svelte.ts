// How a list is drawn, which is not the same thing as how it is organized.
//
// **Organization is yours and lives on disk**: the load order in ModsConfig.xml, which group a
// mod is in, what a group takes in by itself, where its members sort. Changing any of that
// changes what the game loads.
//
// **Layout is this window on this machine**: whether the rows are drawn in one run or broken
// into phases. Changing it changes nothing anybody else can see, and nothing a later scan can
// disagree with. So it lives in localStorage beside the paper, and never in user data -- a list
// moved between machines must not drag a layout with it, for the same reason it must not drag a
// theme.
//
// That separation is the whole point of this file. `settings.listByPhase` used to be the only
// control, it lived in the same file as the groups, and it applied only to the load order. So
// "show me the phases for a second" was a write to the file that holds somebody's organization,
// and the split view could not be laid out at all.
//
// Three lists can each be laid out, because they answer different questions and a person can
// want different answers at once: the single list, and the two panes of the library.

export type Layout = "flat" | "phase";
/** Which list is being laid out. `order` is the single list; the other two are the split. */
export type Surface = "order" | "active" | "inactive";

const KEY = "circinus.layout";

interface Stored {
  order: Layout;
  active: Layout;
  inactive: Layout;
  /** Whether the two panes of the split follow each other. */
  linked: boolean;
  /** Set once the old `listByPhase` setting has been read across, so it is read across once. */
  seeded: boolean;
}

const DEFAULTS: Stored = {
  order: "flat",
  active: "phase",
  inactive: "flat",
  // Linked out of the box: reading the same phase opposite itself is what the split is for,
  // and anybody who wants two different layouts is already the kind of person who will find
  // the control.
  linked: true,
  seeded: false
};

function read(): Stored {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return { ...DEFAULTS };
    const v = JSON.parse(raw) as Partial<Stored>;
    const one = (x: unknown, d: Layout): Layout => (x === "flat" || x === "phase" ? x : d);
    return {
      order: one(v.order, DEFAULTS.order),
      active: one(v.active, DEFAULTS.active),
      inactive: one(v.inactive, DEFAULTS.inactive),
      linked: typeof v.linked === "boolean" ? v.linked : DEFAULTS.linked,
      seeded: v.seeded === true
    };
  } catch {
    // A locked-down webview can throw on the accessor itself, and a half-written value parses
    // as nothing. Either way the defaults are a perfectly good answer.
    return { ...DEFAULTS };
  }
}

class Layouts {
  private s = $state<Stored>({ ...DEFAULTS });

  /** Read whatever is remembered. Safe to call before the first snapshot arrives.
   *
   *  Linked-but-differing is not a state the controls can produce, but a hand-edited or
   *  half-written value can be in it, and two panes that claim to be linked while showing
   *  different things is the sort of thing somebody reports as a bug in the link. The active
   *  pane wins, because it is the one with the load order in it. */
  start() {
    const s = read();
    if (s.linked && s.active !== s.inactive) s.inactive = s.active;
    this.s = s;
  }

  /**
   * Take the old setting across, once.
   *
   * `settings.listByPhase` was the single list's layout and lived in user data. Reading it once
   * means nobody's view changes on upgrade; the `seeded` flag means a later edit here is not
   * undone the next time a snapshot arrives. After this the old field is never written again
   * and is left where it is, because deleting a field somebody's older build still reads is how
   * a downgrade loses their setting.
   */
  seedFrom(listByPhase: boolean | undefined) {
    if (this.s.seeded || listByPhase === undefined) return;
    this.s = { ...this.s, order: listByPhase ? "phase" : "flat", seeded: true };
    this.save();
  }

  private save() {
    try {
      localStorage.setItem(KEY, JSON.stringify($state.snapshot(this.s)));
    } catch {
      // Not being able to remember it is not a reason to refuse to change it.
    }
  }

  get linked() {
    return this.s.linked;
  }

  of(surface: Surface): Layout {
    return this.s[surface];
  }

  /** Is this surface drawn in phases? The question almost every caller is actually asking. */
  byPhase(surface: Surface): boolean {
    return this.s[surface] === "phase";
  }

  /**
   * Choose a layout for one surface.
   *
   * While the panes are linked, setting either sets both -- and it is the pane you touched that
   * wins, not the left one by convention, because the pane you were just working in is the one
   * you meant. `order` is the single list and is never linked to anything; there is nothing on
   * screen beside it to agree with.
   */
  set(surface: Surface, layout: Layout) {
    const next = { ...this.s, [surface]: layout };
    if (this.s.linked && surface !== "order") {
      next.active = layout;
      next.inactive = layout;
    }
    this.s = next;
    this.save();
  }

  /**
   * Link or unlink the two panes.
   *
   * Linking adopts `from` -- the pane last touched -- so pressing the button never rearranges
   * the list you were just reading. Unlinking changes nothing at all on screen: it only stops
   * the next change propagating, so the button is never a surprise in either direction.
   */
  setLinked(linked: boolean, from: Surface = "active") {
    const next = { ...this.s, linked };
    if (linked && from !== "order") {
      const l = this.s[from];
      next.active = l;
      next.inactive = l;
    }
    this.s = next;
    this.save();
  }

  toggleLinked(from: Surface = "active") {
    this.setLinked(!this.s.linked, from);
  }
}

export const layouts = new Layouts();
