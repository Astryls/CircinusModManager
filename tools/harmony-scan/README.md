# harmony-scan

Says what a mod's assemblies patch, without running them.

A C# RimWorld mod changes the game by attaching [Harmony](https://github.com/pardeike/Harmony)
patches to its methods. Two mods that *prefix* the same method, or *transpile* the same method,
are the usual reason a pair "doesn't work together" — and normally that only becomes visible in a
crash log, after the fact. This tool reads the metadata and IL of an assembly and reports the
patches it declares, so Circinus can put "who patches what" next to the load order.

Nothing here loads an assembly. Mod assemblies target a runtime this tool is not, and running
somebody else's code to find out what it does is not a thing a mod manager should do — so the
whole scanner is `System.Reflection.Metadata` plus a small IL reader, and a corrupt or native DLL
costs one entry's `error` field rather than the run.

## Running it

```
harmony-scan [--pretty] <assembly-or-folder>...
```

Paths may be `.dll` files or folders to search recursively. The JSON goes to stdout, diagnostics
to stderr, and the exit code is 0 whenever the scan ran at all.

## What it reports, per assembly

- **Attribute patches** — `[HarmonyPatch]` in all its constructor shapes (by `Type`, by type name,
  with a method name, with `argumentTypes` for an overload, with a `MethodType` for a getter,
  setter, constructor or static constructor). Attributes on the class supply defaults and
  attributes on the method refine them field by field, the way Harmony layers them. The patch kind
  comes from `[HarmonyPrefix]`, `[HarmonyPostfix]`, `[HarmonyTranspiler]`, `[HarmonyFinalizer]`,
  `[HarmonyReversePatch]` or the method's name, and `[HarmonyPriority]`, `[HarmonyBefore]` and
  `[HarmonyAfter]` come along with it.
- **Harmony ids** — the string in `new Harmony("…")`, found in IL.
- **Manual patching** — `Patch(…)`, `PatchAll()`, `Unpatch(…)` and friends. When the target is a
  plain `ldstr`/`ldtoken` the tool can resolve, it becomes an ordinary patch entry with
  `"source": "manual"`; when it is computed at runtime, the method that does it is named instead,
  because the honest answer is "something here patches, go and look" rather than silence.
- **Context** — `[StaticConstructorOnStartup]` types and `Verse.Mod` subclasses.

## The JSON contract

Circinus deserializes this in `circinus-core::harmony`; both sides default every field, so an
older tool and a newer app (or the reverse) still understand each other.

```json
{"version":1,"assemblies":[{
  "path":"…/Assemblies/VehicleFramework.dll","name":"VehicleFramework","mvid":"…","error":null,
  "harmonyIds":["smashphil.vehicleframework"],
  "patches":[{"declaringType":"Vehicles.HarmonyPatches","method":"Prefix","kind":"prefix",
              "targetType":"RimWorld.Pawn","targetMethod":"Tick","targetKind":"normal",
              "argumentTypes":null,"priority":600,"before":[],"after":[],"source":"attribute"}],
  "manualPatches":[{"declaringType":"…","method":".cctor","detail":"Patch(…) called with a computed target"}],
  "startupClasses":["Vehicles.VehicleMod"],"modClasses":["Vehicles.VehicleMod"]
}]}
```

## Building

A plain build needs no packages — everything used ships in the .NET shared framework:

```
dotnet build -c Release
```

Publishing does need nuget.org, for the AOT compiler and the runtime pack:

```
dotnet publish -c Release -r win-x64 -p:Aot=true
```

`NuGet.offline.config` clears the package sources for building with no network at all. It is
deliberately not the default: with the sources cleared, a publish fails with a wall of NU1100
"unable to resolve" errors that read like a broken project rather than a missing source.

## Shipping it with Circinus

**Players never build this, and never see .NET.** `npm run release` at the repo root builds the
scanner for the machine it runs on and hands it to Tauri as an `externalBin`, so it travels
inside the installer and lands beside the app's own executable; Circinus looks for it there,
in its data folder, and on `PATH`. `.github/workflows/release.yml` does the same for Windows,
macOS and Linux on a tag.

The script behind it is `scripts/sidecar.mjs`:

```
node scripts/sidecar.mjs                 # this machine, AOT, falling back to self-contained
node scripts/sidecar.mjs --no-aot        # skip AOT (it needs MSVC, or clang and zlib headers)
node scripts/sidecar.mjs --skip-build    # place a binary built elsewhere
```

It writes `src-tauri/binaries/harmony-scan-<target triple>[.exe]` and clears Tauri's cached
copies under `src-tauri/target/`, which it does not refresh on its own — a known way to ship a
stale scanner without noticing.

The main `tauri.conf.json` deliberately does not name the scanner: `externalBin` and
`bundle.resources` both fail the build when the file is absent, which would mean nobody could
compile Circinus without installing the .NET SDK first. `src-tauri/tauri.release.conf.json`
adds it, and `npm run release` merges the two. A build made without it is a build with one
feature turned off — the Patches view says what is missing and how to get it.

## The fixture

`testdata/Fixture` is a stand-in for a mod assembly: it declares its own `HarmonyLib` and `Verse`
types, so it needs no NuGet package and no copy of RimWorld, and the scanner reads it by name
exactly as it reads the real thing. It covers class-level defaults refined per method, an overload
picked out by `argumentTypes`, a property getter, a constructor, a type named by string, priority
and before/after, a Harmony id, a `PatchAll()`, and a `Patch(…)` whose target is computed.

```
dotnet build testdata/Fixture -c Release
dotnet bin/Release/net8.0/harmony-scan.dll --pretty testdata/Fixture/bin/Release/net8.0/FixtureMod.dll
```

Six patches, one Harmony id, two manual entries, one `Verse.Mod` class.
