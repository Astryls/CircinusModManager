// The shapes harmony-scan looks for, declared here so the fixture builds on its own.
namespace HarmonyLib
{
    public enum MethodType { Normal, Getter, Setter, Constructor, StaticConstructor, Enumerator, Async }

    [System.AttributeUsage(System.AttributeTargets.Class | System.AttributeTargets.Method, AllowMultiple = true)]
    public class HarmonyPatch : System.Attribute
    {
        public HarmonyPatch() { }
        public HarmonyPatch(System.Type declaringType) { }
        public HarmonyPatch(System.Type declaringType, string methodName) { }
        public HarmonyPatch(System.Type declaringType, string methodName, params System.Type[] argumentTypes) { }
        public HarmonyPatch(System.Type declaringType, MethodType methodType) { }
        public HarmonyPatch(System.Type declaringType, MethodType methodType, params System.Type[] argumentTypes) { }
        public HarmonyPatch(string methodName) { }
        public HarmonyPatch(string typeName, string methodName) { }
        public HarmonyPatch(MethodType methodType) { }
        public HarmonyPatch(string methodName, MethodType methodType) { }
        public HarmonyPatch(System.Type[] argumentTypes) { }
    }

    [System.AttributeUsage(System.AttributeTargets.Method)] public class HarmonyPrefix : System.Attribute { }
    [System.AttributeUsage(System.AttributeTargets.Method)] public class HarmonyPostfix : System.Attribute { }
    [System.AttributeUsage(System.AttributeTargets.Method)] public class HarmonyTranspiler : System.Attribute { }
    [System.AttributeUsage(System.AttributeTargets.Method)] public class HarmonyFinalizer : System.Attribute { }
    [System.AttributeUsage(System.AttributeTargets.Method)] public class HarmonyReversePatch : System.Attribute { }
    [System.AttributeUsage(System.AttributeTargets.Class)] public class HarmonyPatchAll : System.Attribute { }
    [System.AttributeUsage(System.AttributeTargets.Method)]
    public class HarmonyPriority : System.Attribute { public HarmonyPriority(int priority) { } }
    [System.AttributeUsage(System.AttributeTargets.Method)]
    public class HarmonyBefore : System.Attribute { public HarmonyBefore(params string[] before) { } }
    [System.AttributeUsage(System.AttributeTargets.Method)]
    public class HarmonyAfter : System.Attribute { public HarmonyAfter(params string[] after) { } }

    public class Harmony
    {
        public Harmony(string id) { Id = id; }
        public string Id;
        public void PatchAll() { }
        public void PatchAll(System.Reflection.Assembly asm) { }
        public void Patch(System.Reflection.MethodBase original, object prefix = null, object postfix = null) { }
        public void Unpatch(System.Reflection.MethodBase original, System.Reflection.MethodInfo patch) { }
        public void UnpatchAll(string id) { }
    }
}
namespace Verse
{
    public class Mod { }
    [System.AttributeUsage(System.AttributeTargets.Class)] public class StaticConstructorOnStartup : System.Attribute { }
}
namespace RimWorld
{
    public class Pawn { public int HitPoints { get; set; } public Pawn() { } public void Tick() { } public void Tick(int n) { } }
    public class Building { public void SpawnSetup() { } }
}
