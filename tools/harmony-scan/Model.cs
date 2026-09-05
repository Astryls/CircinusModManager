namespace Circinus.HarmonyScan;

/// <summary>
/// One scanned .dll. <see cref="Error"/> is non-null when the file could not be read at all;
/// every other field is then empty. A managed assembly that simply has nothing to do with
/// Harmony still gets an entry with empty lists — that is a useful answer, and it lets the
/// Rust side cache "nothing here" instead of re-opening the file on every scan.
/// </summary>
internal sealed class AssemblyResult
{
    public string Path = "";
    public string? Name;
    public string? Mvid;
    public string? Error;
    public List<string> HarmonyIds = new();
    public List<PatchEntry> Patches = new();
    public List<ManualEntry> ManualPatches = new();
    public List<string> StartupClasses = new();
    public List<string> ModClasses = new();
}

/// <summary>A patch method and the game method it attaches to.</summary>
internal sealed class PatchEntry
{
    /// <summary>Type that declares the patch method, e.g. <c>Vehicles.HarmonyPatches</c>.</summary>
    public string DeclaringType = "";
    /// <summary>Name of the patch method itself, e.g. <c>Prefix</c>.</summary>
    public string Method = "";
    /// <summary>prefix | postfix | transpiler | finalizer | reverse | patch | unpatch.</summary>
    public string Kind = "";
    public string? TargetType;
    public string? TargetMethod;
    /// <summary>Decoded <c>MethodType</c>: normal | getter | setter | constructor | staticConstructor | enumerator | async.</summary>
    public string TargetKind = "normal";
    /// <summary>Declared overload disambiguation, or null when the attribute did not give one.</summary>
    public List<string>? ArgumentTypes;
    public int? Priority;
    public List<string> Before = new();
    public List<string> After = new();
    /// <summary>attribute | manual.</summary>
    public string Source = "attribute";
}

/// <summary>
/// A place that patches at runtime in a way static metadata cannot follow — a computed
/// target, a <c>TargetMethod()</c> resolver, a bare <c>PatchAll()</c>. Named so a human can
/// go and look, because the alternative is pretending the patch does not exist.
/// </summary>
internal sealed class ManualEntry
{
    public string DeclaringType = "";
    public string Method = "";
    public string Detail = "";
}

/// <summary>
/// The pieces a <c>[HarmonyPatch]</c> attribute can carry. Harmony builds a target by
/// layering these: the attributes on the class supply defaults, the attributes on the patch
/// method refine them field by field, and several attributes on one member combine. Every
/// field is nullable precisely so "not stated" and "stated as empty" stay distinguishable.
/// </summary>
internal struct PatchSpec
{
    public string? DeclaringType;
    public string? MethodName;
    public int? MethodType;
    public List<string>? ArgumentTypes;

    /// <summary>Layer <paramref name="over"/> on top of this one; anything it states wins.</summary>
    public void Apply(PatchSpec over)
    {
        if (over.DeclaringType is not null) DeclaringType = over.DeclaringType;
        if (over.MethodName is not null) MethodName = over.MethodName;
        if (over.MethodType is not null) MethodType = over.MethodType;
        if (over.ArgumentTypes is not null) ArgumentTypes = over.ArgumentTypes;
    }

    public readonly bool IsEmpty => DeclaringType is null && MethodName is null && MethodType is null && ArgumentTypes is null;
}

/// <summary>Priority / ordering hints, which layer the same way a <see cref="PatchSpec"/> does.</summary>
internal struct PatchOrdering
{
    public int? Priority;
    public List<string>? Before;
    public List<string>? After;

    public void Apply(PatchOrdering over)
    {
        if (over.Priority is not null) Priority = over.Priority;
        if (over.Before is not null) Before = over.Before;
        if (over.After is not null) After = over.After;
    }
}
