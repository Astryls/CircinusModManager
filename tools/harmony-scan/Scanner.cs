using System.Reflection.Metadata;
using System.Reflection.PortableExecutable;

namespace Circinus.HarmonyScan;

/// <summary>
/// Reads one assembly's metadata and reports what it patches.
///
/// Nothing here loads or executes the assembly. Mod DLLs target Unity's Mono and reference
/// <c>Assembly-CSharp</c>, so they could not be loaded here anyway — but the real reason is
/// that a mod manager must never run a mod's static constructors on the player's machine just
/// to answer a question about it.
/// </summary>
internal static class Scanner
{
    /// <summary>Patch-method names Harmony recognises without an attribute.</summary>
    private static readonly Dictionary<string, string> KindByName = new(StringComparer.OrdinalIgnoreCase)
    {
        ["Prefix"] = "prefix",
        ["Postfix"] = "postfix",
        ["Transpiler"] = "transpiler",
        ["Finalizer"] = "finalizer",
    };

    public static AssemblyResult Scan(string path)
    {
        var result = new AssemblyResult { Path = path };
        try
        {
            using var stream = File.OpenRead(path);
            // Prefetching the whole image keeps the IL readable after the stream work is done
            // and costs nothing worth counting: mod assemblies are tens of kilobytes.
            using var pe = new PEReader(stream, PEStreamOptions.PrefetchEntireImage);
            if (!pe.HasMetadata)
            {
                result.Error = "not a managed assembly (no CLI metadata)";
                return result;
            }
            var mr = pe.GetMetadataReader();
            ReadIdentity(mr, result);
            ReadTypes(pe, mr, result);
        }
        catch (BadImageFormatException e)
        {
            result.Error = "unreadable metadata: " + e.Message;
        }
        catch (IOException e)
        {
            result.Error = "cannot read file: " + e.Message;
        }
        catch (UnauthorizedAccessException e)
        {
            result.Error = "cannot read file: " + e.Message;
        }
        catch (InvalidOperationException e)
        {
            // PEReader raises this for images it cannot make sense of at all.
            result.Error = "unreadable image: " + e.Message;
        }
        return result;
    }

    private static void ReadIdentity(MetadataReader mr, AssemblyResult result)
    {
        if (mr.IsAssembly)
        {
            result.Name = mr.GetString(mr.GetAssemblyDefinition().Name);
        }
        var module = mr.GetModuleDefinition();
        if (!module.Mvid.IsNil)
        {
            // The MVID changes on every rebuild, so it is the honest identity of "this exact
            // compilation" — handy for the caller's cache, and free to read here.
            result.Mvid = mr.GetGuid(module.Mvid).ToString("D");
        }
    }

    /// <summary>
    /// Whether the assembly has anything to do with Harmony: a reference to 0Harmony, a type
    /// reference into its namespaces, or — for the mods that ILMerge it — its types outright.
    /// Assemblies that fail all three are the common case and skip the expensive work.
    /// </summary>
    private static bool UsesHarmony(MetadataReader mr)
    {
        foreach (var handle in mr.AssemblyReferences)
        {
            var name = mr.GetAssemblyReference(handle).Name;
            if (mr.StringComparer.Equals(name, "0Harmony") || mr.StringComparer.Equals(name, "HarmonyLib") || mr.StringComparer.Equals(name, "Harmony"))
            {
                return true;
            }
        }
        foreach (var handle in mr.TypeReferences)
        {
            if (IsHarmonyNamespace(mr, mr.GetTypeReference(handle).Namespace)) return true;
        }
        foreach (var handle in mr.TypeDefinitions)
        {
            if (IsHarmonyNamespace(mr, mr.GetTypeDefinition(handle).Namespace)) return true;
        }
        return false;
    }

    private static bool IsHarmonyNamespace(MetadataReader mr, StringHandle ns) => mr.StringComparer.Equals(ns, "HarmonyLib") || mr.StringComparer.Equals(ns, "Harmony");

    private static void ReadTypes(PEReader pe, MetadataReader mr, AssemblyResult result)
    {
        var harmony = UsesHarmony(mr);
        var seenManual = new HashSet<string>(StringComparer.Ordinal);

        foreach (var handle in mr.TypeDefinitions)
        {
            TypeDefinition td;
            string full;
            try
            {
                td = mr.GetTypeDefinition(handle);
                full = Names.FullName(mr, handle);
            }
            catch (BadImageFormatException)
            {
                continue;
            }

            // Verse context, cheap and worth having even for assemblies with no Harmony in them.
            if (Names.TypeOf(mr, td.BaseType) == "Verse.Mod") result.ModClasses.Add(full);

            var classSpec = new PatchSpec();
            var classOrder = new PatchOrdering();
            var classHasPatch = false;
            var patchAll = false;

            foreach (var attrHandle in td.GetCustomAttributes())
            {
                var attr = mr.GetCustomAttribute(attrHandle);
                var (_, name) = HarmonyAttributes.Kind(mr, attr);
                switch (name)
                {
                    case "StaticConstructorOnStartupAttribute" or "StaticConstructorOnStartupPriorityAttribute":
                        result.StartupClasses.Add(full);
                        break;
                    case "HarmonyPatchAttribute":
                        classSpec.Apply(HarmonyAttributes.ReadPatch(mr, attr));
                        classHasPatch = true;
                        break;
                    case "HarmonyPatchAllAttribute":
                        patchAll = true;
                        classHasPatch = true;
                        break;
                    case "HarmonyPriorityAttribute":
                        classOrder.Priority = HarmonyAttributes.ReadInt(mr, attr);
                        break;
                    case "HarmonyBeforeAttribute":
                        classOrder.Before = HarmonyAttributes.ReadStrings(mr, attr);
                        break;
                    case "HarmonyAfterAttribute":
                        classOrder.After = HarmonyAttributes.ReadStrings(mr, attr);
                        break;
                }
            }

            if (!harmony) continue;

            foreach (var methodHandle in td.GetMethods())
            {
                try
                {
                    ReadMethod(pe, mr, mr.GetMethodDefinition(methodHandle), full, classSpec, classOrder, classHasPatch, patchAll, result, seenManual);
                }
                catch (BadImageFormatException)
                {
                    // One damaged method row should not lose the rest of the assembly.
                }
            }
        }
    }

    private static void ReadMethod(PEReader pe, MetadataReader mr, MethodDefinition md, string declaringType, PatchSpec classSpec, PatchOrdering classOrder, bool classHasPatch, bool patchAll, AssemblyResult result, HashSet<string> seenManual)
    {
        var methodName = mr.GetString(md.Name);

        var spec = new PatchSpec();
        var order = new PatchOrdering();
        string? kind = null;
        var hasOwnPatchAttribute = false;
        string? computes = null;

        foreach (var attrHandle in md.GetCustomAttributes())
        {
            var attr = mr.GetCustomAttribute(attrHandle);
            var (_, name) = HarmonyAttributes.Kind(mr, attr);
            switch (name)
            {
                case "HarmonyPatchAttribute":
                    spec.Apply(HarmonyAttributes.ReadPatch(mr, attr));
                    hasOwnPatchAttribute = true;
                    break;
                case "HarmonyPrefixAttribute":
                    kind = "prefix";
                    break;
                case "HarmonyPostfixAttribute":
                    kind = "postfix";
                    break;
                case "HarmonyTranspilerAttribute":
                    kind = "transpiler";
                    break;
                case "HarmonyFinalizerAttribute":
                    kind = "finalizer";
                    break;
                case "HarmonyReversePatchAttribute":
                    kind = "reverse";
                    break;
                case "HarmonyPatchAllAttribute":
                    patchAll = true;
                    hasOwnPatchAttribute = true;
                    break;
                case "HarmonyPriorityAttribute":
                    order.Priority = HarmonyAttributes.ReadInt(mr, attr);
                    break;
                case "HarmonyBeforeAttribute":
                    order.Before = HarmonyAttributes.ReadStrings(mr, attr);
                    break;
                case "HarmonyAfterAttribute":
                    order.After = HarmonyAttributes.ReadStrings(mr, attr);
                    break;
                case "HarmonyTargetMethodAttribute":
                case "HarmonyTargetMethodsAttribute":
                    computes = "TargetMethod() picks the patch target at runtime";
                    break;
            }
        }

        // Harmony also finds patch methods by their name alone, inside a class that carries
        // [HarmonyPatch]. The looser suffix rule below (Pawn_Tick_Prefix) is not something
        // Harmony does — but a method that brought its own [HarmonyPatch] and ends in
        // "Prefix" is a prefix in every mod that has ever been written, and saying so is more
        // useful to the reader than dropping the row.
        if (kind is null && (classHasPatch || hasOwnPatchAttribute))
        {
            if (KindByName.TryGetValue(methodName, out var byName)) kind = byName;
            else if (hasOwnPatchAttribute) kind = KindBySuffix(methodName);
        }

        if (computes is not null || (methodName is "TargetMethod" or "TargetMethods" && classHasPatch))
        {
            AddManual(result, seenManual, declaringType, methodName, computes ?? "TargetMethod() picks the patch target at runtime");
        }

        if (kind is not null)
        {
            var merged = classSpec;
            merged.Apply(spec);
            var ordering = classOrder;
            ordering.Apply(order);
            result.Patches.Add(new PatchEntry
            {
                DeclaringType = declaringType,
                Method = methodName,
                Kind = kind,
                TargetType = merged.DeclaringType,
                // [HarmonyPatchAll] means "every method of the target type"; "*" says that
                // plainly and keeps the field a string.
                TargetMethod = merged.MethodName ?? (patchAll ? "*" : null),
                TargetKind = HarmonyAttributes.MethodTypeName(merged.MethodType),
                ArgumentTypes = merged.ArgumentTypes,
                Priority = ordering.Priority,
                Before = ordering.Before ?? new List<string>(),
                After = ordering.After ?? new List<string>(),
                Source = "attribute",
            });
        }

        ReadBody(pe, mr, md, declaringType, methodName, result, seenManual);
    }

    /// <summary>The trailing word of names like <c>Pawn_Tick_Prefix</c>, or null.</summary>
    private static string? KindBySuffix(string methodName)
    {
        foreach (var (suffix, kind) in KindByName)
        {
            if (methodName.Length > suffix.Length && methodName.EndsWith(suffix, StringComparison.OrdinalIgnoreCase)) return kind;
        }
        return null;
    }

    // Harmony's own surface, matched by name so that ILMerged and namespace-rewritten copies
    // (which RimWorld mods really do ship) are still recognised.
    private static bool IsHarmonyRunner(string type) => type is "HarmonyLib.Harmony" or "Harmony.HarmonyInstance" || type.EndsWith(".Harmony", StringComparison.Ordinal) || type.EndsWith(".HarmonyInstance", StringComparison.Ordinal);

    private static bool IsAccessTools(string type) => type == "AccessTools" || type.EndsWith(".AccessTools", StringComparison.Ordinal);

    /// <summary>
    /// Walk a method body for the two things attributes cannot tell us: the Harmony id the mod
    /// registers itself under, and the places it patches by hand.
    ///
    /// The IL is read as a stream, not simulated. That is enough for the shapes mods actually
    /// write — <c>new Harmony("id")</c>, <c>AccessTools.Method(typeof(Pawn), "Tick")</c>,
    /// <c>AccessTools.Method("Verse.Pawn:Tick")</c> — and anything more involved is honestly
    /// reported as a manual patch instead of guessed at.
    /// </summary>
    private static void ReadBody(PEReader pe, MetadataReader mr, MethodDefinition md, string declaringType, string methodName, AssemblyResult result, HashSet<string> seenManual)
    {
        if (md.RelativeVirtualAddress == 0) return;
        byte[] il;
        try
        {
            il = pe.GetMethodBody(md.RelativeVirtualAddress).GetILBytes() ?? Array.Empty<byte>();
        }
        catch (Exception e) when (e is BadImageFormatException or ArgumentOutOfRangeException or InvalidOperationException)
        {
            return;
        }
        if (il.Length == 0) return;

        string? lastString = null;
        string? pendingType = null;
        var resolved = new Queue<(string Type, string Method)>();

        foreach (var (op, operand) in Il.Read(il))
        {
            if (op == Il.Ldstr)
            {
                lastString = Il.UserString(mr, operand);
                continue;
            }
            if (op == Il.Ldtoken)
            {
                // typeof(X) is `ldtoken X; call Type.GetTypeFromHandle`; the GetTypeFromHandle
                // call is left standing so the type survives until something consumes it.
                pendingType = Il.TokenType(mr, operand) ?? pendingType;
                continue;
            }
            if (op != Il.Call && op != Il.Callvirt && op != Il.Newobj) continue;
            if (Il.Member(mr, operand) is not var (type, name)) continue;

            if (op == Il.Newobj && name == ".ctor" && IsHarmonyRunner(type))
            {
                if (!string.IsNullOrEmpty(lastString) && !result.HarmonyIds.Contains(lastString)) result.HarmonyIds.Add(lastString);
                lastString = null;
                continue;
            }
            if (name == "Create" && IsHarmonyRunner(type))
            {
                // Harmony 1.x: HarmonyInstance.Create("id").
                if (!string.IsNullOrEmpty(lastString) && !result.HarmonyIds.Contains(lastString)) result.HarmonyIds.Add(lastString);
                lastString = null;
                continue;
            }

            if (IsAccessTools(type) || type == "System.Type" || type == "System.Reflection.TypeInfo")
            {
                var target = ResolveTarget(name, ref pendingType, ref lastString);
                if (target is not null) resolved.Enqueue(target.Value);
                continue;
            }

            if (!IsHarmonyRunner(type)) continue;

            switch (name)
            {
                case "Patch" or "PatchCategory" or "CreateReversePatcher" or "ReversePatch" or "Unpatch":
                {
                    var kind = name switch
                    {
                        "CreateReversePatcher" or "ReversePatch" => "reverse",
                        "Unpatch" => "unpatch",
                        _ => "patch",
                    };
                    if (resolved.Count > 0)
                    {
                        var (targetType, targetMethod) = resolved.Dequeue();
                        result.Patches.Add(new PatchEntry
                        {
                            DeclaringType = declaringType,
                            Method = methodName,
                            Kind = kind,
                            TargetType = targetType,
                            TargetMethod = targetMethod,
                            TargetKind = targetMethod == ".ctor" ? "constructor" : "normal",
                            Source = "manual",
                        });
                    }
                    else
                    {
                        AddManual(result, seenManual, declaringType, methodName, name + "(…) called with a computed target");
                    }
                    break;
                }
                case "PatchAll" or "PatchAllUncategorized":
                    AddManual(result, seenManual, declaringType, methodName, "PatchAll() applies every [HarmonyPatch] in the assembly");
                    break;
                case "UnpatchAll" or "UnpatchCategory":
                    AddManual(result, seenManual, declaringType, methodName, name + "() removes patches at runtime");
                    break;
            }
        }
    }

    /// <summary>
    /// Turn the reflection call the mod just made into a (type, method) pair, consuming the
    /// operands it used. Null when the call was not one that names a target.
    /// </summary>
    private static (string Type, string Method)? ResolveTarget(string name, ref string? pendingType, ref string? lastString)
    {
        switch (name)
        {
            case "Constructor" or "DeclaredConstructor" or "GetConstructor":
            {
                if (pendingType is null) return null;
                var target = (pendingType, ".ctor");
                pendingType = null;
                return target;
            }
            case "Method" or "DeclaredMethod" or "GetMethod" or "PropertyGetter" or "DeclaredPropertyGetter" or "PropertySetter" or "DeclaredPropertySetter" or "GetProperty" or "EnumeratorMoveNext":
            {
                // AccessTools.Method("Verse.Pawn:Tick") names both halves in one string.
                if (pendingType is null && lastString is not null && lastString.Contains(':'))
                {
                    var parts = lastString.Split(':', 2);
                    lastString = null;
                    return parts[0].Length == 0 || parts[1].Length == 0 ? null : (parts[0], parts[1]);
                }
                if (pendingType is null || lastString is null) return null;
                var target = (pendingType, lastString);
                pendingType = null;
                lastString = null;
                return target;
            }
            default:
                return null;
        }
    }

    private static void AddManual(AssemblyResult result, HashSet<string> seen, string declaringType, string method, string detail)
    {
        if (!seen.Add(declaringType + "\0" + method + "\0" + detail)) return;
        result.ManualPatches.Add(new ManualEntry { DeclaringType = declaringType, Method = method, Detail = detail });
    }
}
