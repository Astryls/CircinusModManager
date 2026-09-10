// The English catalogue: the source text, and the fallback for every other locale.
//
// Grouped by where the string is read, and in the order it appears there, so a translator works
// down a screen rather than jumping about. Keep the grouping when adding: a catalogue sorted by
// key alone is unreadable to the person who has to translate it.
//
// A `{ one, other }` value is a plural; `{n}` and `{name}` are filled at the call site.

import type { Catalogue } from "../i18n.svelte";

export const EN: Catalogue = {
  // ---- the eight phases, and what each is for -------------------------------------------
  "phase.prepatch.name": "Preloads",
  "phase.prepatch.note": "Harmony, Prepatcher, loaders: before the game itself",
  "phase.core.name": "Game and DLC",
  "phase.core.note": "RimWorld's own content",
  "phase.framework.name": "Libraries",
  "phase.framework.note": "Other mods build on these",
  "phase.content.name": "Content",
  "phase.content.note": "Things, pawns, biomes, rules",
  "phase.patch.name": "Patches",
  "phase.patch.note": "Load after the mods they change",
  "phase.texture.name": "Texture packs",
  "phase.texture.note": "The later pack wins",
  "phase.late.name": "Late loaders",
  "phase.late.note": "Asked to load near the bottom, with their add-ons",
  "phase.optimization.name": "Performance",
  "phase.optimization.note": "Load last to see everything",

  // ---- where a mod came from --------------------------------------------------------------
  // Ludeon, Steam, SteamCMD and Git are names, not words, and stay as they are in every locale.
  "source.ludeon": "Ludeon",
  "source.workshop": "Steam",
  "source.local": "Local",
  "source.steamcmd": "SteamCMD",
  "source.git": "Git",

  // ---- measurement bands ------------------------------------------------------------------
  "band.negligible": "Negligible",
  "band.light": "Light",
  "band.moderate": "Moderate",
  "band.heavy": "Heavy",
  "band.veryheavy": "Very heavy",
  "band.insufficient": "Few runs",
  "band.unknown": "Not measured",

  // ---- what changed about a mod since last time -------------------------------------------
  "change.reason.workshopUpdate": "updated on the Workshop",
  "change.reason.versionChange": "new version",
  "change.reason.filesChanged": "files changed",
  "change.reason.renamed": "renamed",
  "change.reason.sourceChanged": "comes from a different place now",

  // ---- ways to order the list --------------------------------------------------------------
  "sort.order.label": "Load order",
  "sort.order.hint": "The order the game will load them in. The only real one",
  "sort.name.label": "Name",
  "sort.name.hint": "A to Z",
  "sort.pkg.label": "Package id",
  "sort.pkg.hint": "A to Z",
  "sort.versions.label": "Game version",
  "sort.versions.hint": "The newest version each mod says it supports, so the ones furthest behind gather at one end",
  "sort.time.label": "Loading time",
  "sort.time.hint": "Seconds each mod is expected to add to the loading bar",
  "sort.load.label": "Share of loading",
  "sort.load.hint": "The same estimate as a share of the list",
  "sort.cost.label": "Frame time",
  "sort.cost.hint": "Share of frame time from circinus.sh, where there is a measurement",
  "sort.phase.label": "Phase",
  "sort.phase.hint": "Where HALO files each mod",
  "sort.group.label": "Group",
  "sort.group.hint": "A to Z by group name",
  "sort.arrived.label": "Date added",
  "sort.arrived.hint": "When Circinus first saw the folder",
  "sort.modified.label": "Date modified",
  "sort.modified.hint": "When the files on disk last changed. For a Workshop mod that includes what Steam replaced",
  "sort.updated.label": "Date updated on Steam",
  "sort.updated.hint": "When the author last published an update. Local mods have no such date and sort last",
  "sort.steamid.label": "Steam id",
  "sort.steamid.hint": "The Workshop item number. Local mods have none and sort last",

  // ---- the toolbar above the list ----------------------------------------------------------
  "toolbar.tab.active": "Active",
  "toolbar.tab.inactive": "Inactive",
  "toolbar.tab.all": "All",
  "toolbar.tab.new": "New",
  "toolbar.arrangement": "Arrangement",
  "toolbar.byPhase": "By phase",
  "toolbar.plain": "Plain",
  "toolbar.byPhase.title": "Gather the list under phase headings",
  "toolbar.plain.title": "One list, in load order, with no headings",
  "toolbar.split.title": "Show inactive and active mods in two panels, and drag between them",
  "toolbar.show.title": "Narrow the list to mods with errors, warnings, conflicts, HALO notes or changes",
  "toolbar.show.label": "Show: {what}",
  "toolbar.show.only": "Show only",
  "toolbar.show.all": "All mods",
  "toolbar.show.attention": "Needs attention",
  "toolbar.show.error": "With errors",
  "toolbar.show.warning": "With warnings",
  "toolbar.show.conflict": "With conflicts",
  "toolbar.show.collision": "Replacing the same textures",
  "toolbar.show.slow": "Slow to load",
  "toolbar.show.note": "With HALO notes",
  "toolbar.show.changed": "Changed since last launch",
  "toolbar.show.heavy": "Heavy on frame time",
  "toolbar.show.moved": "HALO would move",
  "toolbar.columns": "Columns",
  "toolbar.from": "From",
  "toolbar.version": "Version",
  "toolbar.onlyThisVersion": "Only mods made for {version}",
  "toolbar.showEverything": "Show everything",
  "toolbar.sort.title": "Order the list by something other than the load order",
  "toolbar.sort.label": "Sort: {what}",
  "toolbar.sort.by": "Sort by",
  "toolbar.sort.dragNote": "Dragging is refused while the list is sorted: a drop between two rows of a list ordered by name writes a position nobody chose.",
  "toolbar.seenThem": "Seen them",
  "toolbar.seenThem.title": "Stop marking these as new. The dates they arrived stay.",
  "toolbar.sorted": "Sorted",
  "toolbar.sorted.title": "Back to the load order",
  "toolbar.arr.order": "Load order",
  "toolbar.arr.order.short": "Order",
  "toolbar.arr.order.title": "The list exactly as ModsConfig.xml has it, top to bottom",
  "toolbar.arr.phase": "By phase",
  "toolbar.arr.phase.short": "Phase",
  "toolbar.arr.phase.title": "The same mods, gathered under the phase HALO files them in",
  "toolbar.split.label": "Inactive | Active",
  "toolbar.col.cost": "Cost",
  "toolbar.col.cost.title": "Share of frame time, once weights are loaded",
  "toolbar.col.time": "Time",
  "toolbar.col.time.title": "Seconds this mod is expected to add to the game's loading time",
  "toolbar.col.load": "Load",
  "toolbar.col.load.title": "Expected share of the list's loading time",
  "toolbar.col.versions": "Versions",
  "toolbar.col.versions.title": "Game versions the mod says it supports",
  "toolbar.col.phase": "Phase",
  "toolbar.col.phase.title": "Where HALO files the mod — already the sections when the list is arranged by phase",
  "toolbar.col.group": "Group",
  "toolbar.col.group.title": "The group the mod is in",
  "toolbar.refresh": "Refresh",
  "toolbar.refresh.title": "Read the mod folders again",
  "toolbar.whatChanges": "What changes",
  "toolbar.whatChanges.title": "Show which mods HALO would move, from where to where",
  "toolbar.apply": "Apply",
  "toolbar.apply.moves": { one: " {n} move", other: " {n} moves" },
  "toolbar.apply.title": { one: "Apply the {n} move HALO proposes", other: "Apply the {n} moves HALO proposes" },
  "toolbar.discard": "Discard",
  "toolbar.discard.title": "Discard the preview; nothing moves",
  "toolbar.discard.aria": "Discard the preview",
  "toolbar.sortWithHalo": "Sort with HALO",
  "toolbar.sortWithHalo.title": "Preview the load order HALO would use, then apply it or discard it",
  "toolbar.import": "Import",
  "toolbar.import.title": "Import a mod list ({keys})",

  // ---- the sidebar --------------------------------------------------------------------------
  "rail.views": "Views",
  "rail.nav.order": "Load order",
  "rail.nav.library": "Library",
  "rail.nav.downloads": "Downloads",
  "rail.nav.textures": "Textures",
  "rail.nav.patches": "Patches",
  "rail.nav.analyzer": "Analyzer",
  "rail.nav.defs": "Defs",
  "rail.nav.halo": "HALO",
  "rail.nav.settings": "Settings",
  "rail.help": "Help on Discord",
  "rail.help.title": "Ask for help, or say a mod sorted somewhere odd",

  // ---- keyboard shortcuts and the command palette ---------------------------------------------
  // These name actions, so they read as commands: "Save the mod list", not "Saving" or "Save?".
  // The chords themselves are not translated -- Ctrl is Ctrl everywhere -- and the labels are
  // built from the binding, so a Mac shows the Command symbol without a second string here.
  "keys.section.list": "Your list",
  "keys.section.review": "Problems",
  "keys.section.go": "Getting around",
  "keys.save": "Save the mod list",
  "keys.search": "Search mods",
  "keys.import": "Import a list",
  "keys.refresh": "Read the mod folders again",
  "keys.play": "Play",
  "keys.halo": "Preview the order HALO would use",
  "keys.halo.apply": "Apply the previewed order",
  "keys.halo.discard": "Discard the preview",
  "keys.selectAll": "Select everything on screen",
  "keys.deactivate": "Deactivate the selection",
  "keys.moveUp": "Move the selection up",
  "keys.moveDown": "Move the selection down",
  "keys.reviewNext": "Go to the next problem",
  "keys.reviewPrev": "Go to the previous problem",
  "keys.palette": "Find a command",
  "keys.shortcuts": "Keyboard shortcuts",
  "keys.view": "Go to {name}",

  "titlebar.downloads.aria": "Downloads ({keys})",
  "titlebar.save.title": "Write ModsConfig.xml ({keys})",

  "palette.title": "Find a command",
  "palette.placeholder": "Type what you want to do",
  "palette.none": "Nothing matches that.",
  "palette.hint": "Enter runs it. Escape closes.",
  "palette.unavailable": "not available right now",

  "shortcuts.title": "Keyboard shortcuts",
  "shortcuts.lead": "Anything with a name can also be found by pressing {palette}.",
  "shortcuts.section.inlist": "In the list",
  "shortcuts.list": "In the list, with a row selected: arrow keys move, Shift and an arrow extends the selection, Home and End jump to the ends, Page Up and Page Down move a screenful, Enter activates or deactivates.",

  // ---- what a modpack's curator said ---------------------------------------------------------
  // These words wrap somebody else's writing. "Pack" is the curator's Steam collection; keep the
  // author's name and the pack's name distinct, because the whole point of the screen is that a
  // player can tell whose words they are reading.
  "packs.banner.title": { one: "{n} update from a modpack you follow", other: "{n} updates from modpacks you follow" },
  "packs.banner.detail": "Posted by {name}",
  "packs.banner.detail.many": "From {names}",
  "packs.banner.action": "Read",
  "packs.title": "Modpack updates",
  "packs.close": "Close",
  "packs.lead": "Written by the people who curate the packs you follow, and shown here as they wrote it. Circinus passes these along and does not check them: treat anything asking you to install, delete or move files the way you would treat a message from a stranger.",
  "packs.empty": "Nothing from your curators yet.",
  "packs.empty.none": "Follow a Steam collection from the sidebar and anything its curator posts turns up here.",
  "packs.unread": "New",
  "packs.checked": "Checked {when}",
  "packs.checked.never": "Not asked yet",
  "packs.refresh": "Check now",
  "packs.markread": "Mark all read",
  "packs.open": "Open link",
  "packs.mute": "Mute this curator",
  "packs.unmute": "Unmute",
  "packs.muted.note": "Muted. You still follow the pack; you will not see what its curator posts.",
  "packs.muted": "Muted {name}. You still follow the pack.",
  "packs.unmuted": "You will hear from {name} again.",
  "packs.by": "{name}, curator of {pack}",

  // ---- the patch report ------------------------------------------------------------------------
  // Four questions, four tabs. "Contested" is the word the report uses for a method two mods both
  // want to decide; if a language has a better one for a tug of war, use that rather than a
  // literal translation.
  "patches.title": "Patches",
  "patches.status.unread": "not read yet",
  "patches.status.took": "read in {n}s",
  "patches.read": "Read the assemblies",
  "patches.reread": "Read them again",
  "patches.stop": "Stop",
  "patches.finding": "Finding assemblies…",
  "patches.reading": "Reading {done} of {total} assemblies",
  "patches.lastrun": "Last run read {n} assemblies in {mods} mods in {seconds}s.",
  "patches.lastrun.stopped": "Last run read {n} assemblies in {mods} mods, then stopped, in {seconds}s.",

  "patches.tab.overview": "Overview",
  "patches.tab.contested": "Two mods over one method",
  "patches.tab.methods": "Every patched method",
  "patches.tab.permod": "Per mod",
  "patches.tab.manual": "Cannot be followed",

  "patches.tile.contested": "Contested methods",
  "patches.tile.contested.some": "two or more mods change the same method",
  "patches.tile.contested.none": "nothing two mods fight over",
  "patches.tile.methods": "Patched methods",
  "patches.tile.methods.sub": "game methods your active mods attach to",
  "patches.tile.mods": "Mods with code",
  "patches.tile.mods.sub": "of {n} active",
  "patches.tile.patches": "Patches in all",
  "patches.tile.patches.sub": "{n} more Circinus cannot follow",

  "patches.about.title": "What your mods patch",
  "patches.about.lead":
    "Circinus reads every active mod's assemblies itself and reports the game methods they attach to. Nothing is loaded or run: it reads the metadata and instructions of a file the way a disassembler does, so a mod's code never executes. Results are kept until a mod changes, so a second run is quick.",

  // The three emphasised phrases are inside the sentence rather than interpolated around it, so a
  // translator can put the emphasis where their own word order needs it. Rendered with `{@html}`;
  // `<b>` is the only tag any entry here is allowed to carry.
  "patches.contested.lead":
    "A method is contested when two mods <b>run before</b> it or <b>rewrite</b> it: only one of them decides what the original does, so one usually loses. Mods that only <b>run after</b> a method stack cleanly and are not counted here.",
  "patches.contested.none": "Nothing here. Every method your active mods patch is either patched by one mod, or only added to.",
  "patches.contested.priority": "Harmony priority; higher goes first",

  "patches.methods.filter": "Filter by method or mod",
  "patches.methods.shown": { one: "{n} shown", other: "{n} shown" },
  "patches.methods.col.method": "Method",
  "patches.methods.col.mods": "Mods",
  "patches.methods.col.by": "Patched by",
  "patches.methods.nomatch": "No method or mod matches that.",
  "patches.methods.none": "Nothing patched: no active mod ships code.",

  "patches.permod.lead":
    "Click a mod to see the methods it patches. A Harmony id is the name a mod gives its own patches; the game's logs use it to say whose patch threw.",
  "patches.permod.col.mod": "Mod",
  "patches.permod.col.patches": "Patches",
  "patches.permod.col.before": "Before",
  "patches.permod.col.after": "After",
  "patches.permod.col.rewrites": "Rewrites",
  "patches.permod.col.unreadable": "Unreadable",
  "patches.permod.col.harmonyid": "Harmony id",
  "patches.permod.noid": "none found",
  "patches.permod.reading": "Reading…",
  "patches.permod.unnamed": "No patch this tool can name: everything it does happens at runtime.",
  "patches.permod.contested": { one: "In {n} contested method: {names}", other: "In {n} contested methods: {names}" },
  "patches.permod.none": "No active mod ships an assembly.",

  "patches.manual.lead":
    "A mod can work out at runtime which method to patch — from a setting, a name it builds, or the mods it finds installed. A target computed like that is not written down in the file, so reading it cannot say what it will be. These are the places that do it, so you know where to look when something goes wrong there.",
  "patches.manual.none": "Every patch your active mods declare names its target outright.",
  "patches.manual.places": { one: "{n} place", other: "{n} places" },
  "patches.manual.unreadable": { one: "{n} assembly could not be read at all", other: "{n} assemblies could not be read at all" },

  // What a patch does to the method it is attached to. Harmony's own words are prefix, postfix
  // and transpiler; these say what those mean instead, because a player is not reading a manual.
  "patches.kind.prefix": "runs before",
  "patches.kind.postfix": "runs after",
  "patches.kind.transpiler": "rewrites",
  "patches.kind.finalizer": "catches errors in",
  "patches.kind.reverse": "copies",
  "patches.kind.patch": "patches",
  "patches.kind.unpatch": "unpatches",

  // ---- counts, in the two forms English needs ------------------------------------------------
  "count.mods": { one: "{n} mod", other: "{n} mods" },
  "count.edits": { one: "{n} edit", other: "{n} edits" },
  "count.files": { one: "{n} file", other: "{n} files" }
};
