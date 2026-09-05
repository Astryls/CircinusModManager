# harmony-scan — the test oracle

**This is not part of Circinus.** It is a reference implementation, kept so that the Rust
assembly reader in `circinus-core::clr` can be checked against something written independently.
Nothing in the app runs it, no build needs it, and a player never sees it. If you are looking for
the code that actually reads assemblies, it is `crates/circinus-core/src/clr/`.

It exists because a from-scratch ECMA-335 reader is easy to get subtly wrong, and "subtly wrong"
here means quietly telling somebody two mods are fine together when they are not. This tool uses
.NET's own `System.Reflection.Metadata` to answer the same questions, and the Rust reader must
agree with it exactly.

## Why the app does not use it any more

It shipped as a sidecar: a NativeAOT binary built by `dotnet publish` and carried inside the
installer. That worked, but it put the .NET SDK between a contributor and a working build, and
every seam in it — NuGet sources, the AOT toolchain, which folder cargo used, whether the
binary was stale — was a way for the Patches view to be silently empty. Reading ECMA-335 needs
no runtime, so the reader moved into `circinus-core` and the seams went with it.

The Rust reader also sees more. Frameworks like Vehicle Framework and SmashTools patch through
their own helpers rather than calling `Harmony.Patch`, which this tool cannot follow: it reports
zero patches for `Vehicles.dll`. The Rust reader also records `AccessTools` lookups, which finds
266 real targets in the same file.

## Running it

```
dotnet build -c Release                       # needs the .NET 8 SDK and nuget.org
dotnet bin/Release/net8.0/harmony-scan.dll --pretty <assembly-or-folder>...
```

`NuGet.offline.config` clears the package sources for a build with no network. Do not make it the
default: with the sources cleared, publishing fails with NU1100 errors that read like a broken
project rather than a missing source.

## Refreshing the oracle

`crates/circinus-core/tests/clr/` holds a few assemblies and, beside each, this tool's answer as
`<name>.expected.json`. `crates/circinus-core/tests/clr_reader.rs` asserts the Rust reader
matches them field by field. To add an assembly to the set, or to refresh one after teaching this
tool something new:

```
dotnet bin/Release/net8.0/harmony-scan.dll --pretty path/to/Some.dll > \
  ../../crates/circinus-core/tests/clr/Some.expected.json
cp path/to/Some.dll ../../crates/circinus-core/tests/clr/
```

Only add assemblies whose licence allows redistribution: the set holds `FixtureMod.dll` (ours),
and Harmony's `0Harmony.dll` and `HarmonyMod.dll` (MIT, pardeike/HarmonyRimWorld). To check
against mods you have installed without committing them, point the ignored test at a folder:

```
CIRCINUS_CLR_REAL=/path/to/folder cargo test -p circinus-core --test clr_reader \
  real_assemblies_report -- --ignored --nocapture
```

The folder wants each `Some.dll` beside its `Some.expected.json`. The test prints timings, what
each file yielded, and any field where the two disagree.

## The fixture

`testdata/Fixture` is a stand-in for a mod assembly: it declares its own `HarmonyLib` and `Verse`
types, so it needs no NuGet package and no copy of RimWorld. It covers class-level defaults
refined per method, an overload picked out by `argumentTypes`, a property getter, a constructor,
a type named by string, priority and before/after, a Harmony id, a `PatchAll()`, and a `Patch(…)`
whose target is computed. Six patches, one Harmony id, two manual entries, one `Verse.Mod` class.

```
dotnet build testdata/Fixture -c Release
dotnet bin/Release/net8.0/harmony-scan.dll --pretty testdata/Fixture/bin/Release/net8.0/FixtureMod.dll
```

## What it reports

Per assembly: `[HarmonyPatch]` attributes in all their constructor shapes (class-level supplying
defaults that method-level attributes refine), the patch kind and `[HarmonyPriority]`,
`[HarmonyBefore]`, `[HarmonyAfter]`; Harmony ids from `new Harmony("…")` in IL; manual
`Patch`/`PatchAll`/`Unpatch` calls, resolved when the target is a plain `ldstr`/`ldtoken` and
named as unfollowable when it is computed; `[StaticConstructorOnStartup]` types and `Verse.Mod`
subclasses. The JSON shape is in `Model.cs` and is what `circinus-core::harmony` deserializes.
