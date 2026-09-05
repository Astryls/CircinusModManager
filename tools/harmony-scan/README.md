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

A plain build needs no network: everything used ships in the .NET shared framework, and
`NuGet.config` clears the package sources so a stray dependency fails the build rather than
appearing quietly.

```
dotnet build -c Release
```

Circinus ships the tool as a single native binary beside the app — no .NET runtime on the
player's machine, no JIT, a few milliseconds to start. The AOT compiler and the trimmer arrive as
NuGet packages, so they are asked for only when you publish with `-p:Aot=true`, and that step
does need network (and, on Linux, `clang` and `zlib` development headers):

```
dotnet publish -c Release -r win-x64   -p:Aot=true
dotnet publish -c Release -r linux-x64 -p:Aot=true
dotnet publish -c Release -r osx-arm64 -p:Aot=true
```

Each writes `bin/Release/net8.0/<rid>/publish/harmony-scan(.exe)`. Circinus looks for that binary
beside its own executable, in its data folder, and on `PATH`.

## Shipping it with Circinus

**This is a release step, not part of the app build.** Publish the scanner for the platform being
released and copy it next to the bundled executable before the installer is made:

```
dotnet publish -c Release -r win-x64 -p:Aot=true
copy bin\Release\net8.0\win-x64\publish\harmony-scan.exe ..\..\src-tauri\target\release\
```

`src-tauri/tauri.conf.json` deliberately does not name the scanner. Tauri's two ways of carrying
an extra file both make its absence fatal: `bundle.externalBin` wants
`harmony-scan-<target-triple>.exe` and fails the build when that file is not there, and
`bundle.resources` fails just as hard — a glob that matches nothing is
`GlobPathNotFound`, not "nothing to copy". Naming it either way would mean nobody could build
Circinus without first installing the .NET SDK and publishing this tool, which needs a network,
and on Linux `clang` and zlib headers besides. So the copy stays here, in the release notes,
where it costs one line.

When publishing the scanner becomes part of the release script, add it as a resource then:

```json
"bundle": { "resources": ["harmony-scan.exe"] }
```

with the path relative to `src-tauri`, and the file present before `tauri build` runs. A player
who has no scanner sees the Patches view explain what it is and how to get one, so a build
without it is a build with one feature turned off rather than a broken one.

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
