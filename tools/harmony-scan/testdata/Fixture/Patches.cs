using System.Reflection;
using HarmonyLib;
using RimWorld;
using Verse;

namespace FixtureMod
{
    // A class-level attribute supplies the defaults; each method refines them.
    [HarmonyPatch(typeof(Pawn), "Tick")]
    public static class PawnTickPatch
    {
        [HarmonyPrefix]
        [HarmonyPriority(600)]
        [HarmonyBefore("other.mod")]
        static bool Before() => true;

        [HarmonyPostfix]
        [HarmonyAfter("another.mod")]
        static void After() { }
    }

    // Overloads picked out by argument types, on the method attribute rather than the class.
    public static class OverloadPatch
    {
        [HarmonyPatch(typeof(Pawn), "Tick", new[] { typeof(int) })]
        [HarmonyTranspiler]
        static void Transpile() { }
    }

    // A property getter and a constructor.
    [HarmonyPatch(typeof(Pawn))]
    public static class PropertyAndCtor
    {
        [HarmonyPatch("HitPoints", MethodType.Getter)]
        [HarmonyPostfix]
        static void Getter() { }

        [HarmonyPatch(MethodType.Constructor)]
        [HarmonyPrefix]
        static void Ctor() { }
    }

    // A finalizer on a type named by string, as mods do for types they cannot reference.
    public static class ByName
    {
        [HarmonyPatch("RimWorld.Building", "SpawnSetup")]
        [HarmonyFinalizer]
        static void Fin() { }
    }

    // The mod entry point: a Harmony id, a PatchAll, and one target the metadata cannot follow.
    [StaticConstructorOnStartup]
    public class FixtureModEntry : Mod
    {
        static FixtureModEntry()
        {
            var h = new Harmony("fixture.mod.id");
            h.PatchAll();
            h.Patch(Resolve(), null, null);
        }
        static MethodBase Resolve() => typeof(Pawn).GetMethod("Tick");
    }
}
