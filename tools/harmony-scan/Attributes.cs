using System.Collections.Immutable;
using System.Reflection.Metadata;

namespace Circinus.HarmonyScan;

/// <summary>
/// Decodes custom attribute blobs into plain type <em>names</em>. The blob format stores
/// arguments by signature, so something has to say what each encoded type is; normally that
/// something is the runtime loader, but this tool must never load a mod assembly (they target
/// Unity's Mono and running their static constructors on a player's machine is not a risk
/// worth taking). Returning strings keeps the whole decode reflection-free.
/// </summary>
internal sealed class TypeNameProvider : ICustomAttributeTypeProvider<string>
{
    public static readonly TypeNameProvider Instance = new();

    public string GetPrimitiveType(PrimitiveTypeCode typeCode) => typeCode switch
    {
        PrimitiveTypeCode.Boolean => "System.Boolean",
        PrimitiveTypeCode.Byte => "System.Byte",
        PrimitiveTypeCode.Char => "System.Char",
        PrimitiveTypeCode.Double => "System.Double",
        PrimitiveTypeCode.Int16 => "System.Int16",
        PrimitiveTypeCode.Int32 => "System.Int32",
        PrimitiveTypeCode.Int64 => "System.Int64",
        PrimitiveTypeCode.SByte => "System.SByte",
        PrimitiveTypeCode.Single => "System.Single",
        PrimitiveTypeCode.String => "System.String",
        PrimitiveTypeCode.UInt16 => "System.UInt16",
        PrimitiveTypeCode.UInt32 => "System.UInt32",
        PrimitiveTypeCode.UInt64 => "System.UInt64",
        PrimitiveTypeCode.Object => "System.Object",
        PrimitiveTypeCode.TypedReference => "System.TypedReference",
        PrimitiveTypeCode.IntPtr => "System.IntPtr",
        PrimitiveTypeCode.UIntPtr => "System.UIntPtr",
        PrimitiveTypeCode.Void => "System.Void",
        _ => "System.Object",
    };

    public string GetSystemType() => "System.Type";

    public string GetSZArrayType(string elementType) => elementType + "[]";

    public string GetTypeFromDefinition(MetadataReader reader, TypeDefinitionHandle handle, byte rawTypeKind) => Names.FullName(reader, handle);

    public string GetTypeFromReference(MetadataReader reader, TypeReferenceHandle handle, byte rawTypeKind) => Names.FullName(reader, handle);

    /// <summary>
    /// A <c>System.Type</c> argument arrives as an assembly-qualified name written by the
    /// compiler (<c>"RimWorld.Pawn, Assembly-CSharp, Version=…"</c>). Keep the raw text here;
    /// <see cref="Names.Clean"/> trims it when it reaches the output.
    /// </summary>
    public string GetTypeFromSerializedName(string name) => name ?? "";

    /// <summary>
    /// Every enum Harmony puts in an attribute — <c>MethodType</c>, <c>ArgumentType</c>,
    /// <c>HarmonyReversePatchType</c>, <c>MethodDispatchType</c> — has the default int32
    /// underlying type, and the blob has no other way to tell us. Guessing int32 for an
    /// unknown enum is what every metadata reader does here.
    /// </summary>
    public PrimitiveTypeCode GetUnderlyingEnumType(string type) => PrimitiveTypeCode.Int32;

    public bool IsSystemType(string type) => type.Equals("System.Type", StringComparison.Ordinal) || type.StartsWith("System.Type,", StringComparison.Ordinal);
}

/// <summary>Turning metadata handles into the dotted names a human reads.</summary>
internal static class Names
{
    /// <summary>Full name of a type definition, nested types joined with <c>+</c> as the CLR writes them.</summary>
    public static string FullName(MetadataReader mr, TypeDefinitionHandle handle)
    {
        var td = mr.GetTypeDefinition(handle);
        var name = mr.GetString(td.Name);
        var declaring = td.GetDeclaringType();
        if (!declaring.IsNil)
        {
            return FullName(mr, declaring) + "+" + name;
        }
        var ns = mr.GetString(td.Namespace);
        return ns.Length == 0 ? name : ns + "." + name;
    }

    public static string FullName(MetadataReader mr, TypeReferenceHandle handle)
    {
        var tr = mr.GetTypeReference(handle);
        var name = mr.GetString(tr.Name);
        // A nested type reference has another TypeReference as its resolution scope.
        if (tr.ResolutionScope.Kind == HandleKind.TypeReference)
        {
            return FullName(mr, (TypeReferenceHandle)tr.ResolutionScope) + "+" + name;
        }
        var ns = mr.GetString(tr.Namespace);
        return ns.Length == 0 ? name : ns + "." + name;
    }

    /// <summary>Full name behind any entity handle that can stand for a type, or null.</summary>
    public static string? TypeOf(MetadataReader mr, EntityHandle handle)
    {
        if (handle.IsNil) return null;
        return handle.Kind switch
        {
            HandleKind.TypeDefinition => FullName(mr, (TypeDefinitionHandle)handle),
            HandleKind.TypeReference => FullName(mr, (TypeReferenceHandle)handle),
            // A TypeSpecification is a constructed type (List<Foo>, Foo[]); its blob would have
            // to be decoded with a signature provider. Patch targets are never generic
            // instantiations in practice, so leaving it unnamed loses nothing real.
            _ => null,
        };
    }

    /// <summary>
    /// Drop the assembly qualification from a serialized type name, leaving
    /// <c>RimWorld.Pawn</c>. Commas inside <c>[…]</c> belong to generic arguments and stay.
    /// </summary>
    public static string Clean(string name)
    {
        if (string.IsNullOrEmpty(name)) return name;
        var depth = 0;
        for (var i = 0; i < name.Length; i++)
        {
            var c = name[i];
            if (c == '[') depth++;
            else if (c == ']') depth--;
            else if (c == ',' && depth == 0) return name[..i].Trim();
        }
        return name.Trim();
    }
}

/// <summary>Reading the Harmony attributes off a type or a method.</summary>
internal static class HarmonyAttributes
{
    /// <summary>
    /// Namespace and name of the attribute a <see cref="CustomAttribute"/> constructs. The name
    /// is returned in its <c>…Attribute</c> form whether or not the type carries the suffix:
    /// HarmonyLib declares <c>HarmonyPatch</c>, Verse declares <c>StaticConstructorOnStartup</c>,
    /// and C# lets either spelling appear at a use site, so callers should not have to care.
    /// </summary>
    public static (string Namespace, string Name) Kind(MetadataReader mr, CustomAttribute ca)
    {
        var (ns, name) = RawKind(mr, ca);
        return (ns, name.Length > 0 && !name.EndsWith("Attribute", StringComparison.Ordinal) ? name + "Attribute" : name);
    }

    private static (string Namespace, string Name) RawKind(MetadataReader mr, CustomAttribute ca)
    {
        try
        {
            switch (ca.Constructor.Kind)
            {
                case HandleKind.MethodDefinition:
                {
                    var md = mr.GetMethodDefinition((MethodDefinitionHandle)ca.Constructor);
                    var td = mr.GetTypeDefinition(md.GetDeclaringType());
                    return (mr.GetString(td.Namespace), mr.GetString(td.Name));
                }
                case HandleKind.MemberReference:
                {
                    var member = mr.GetMemberReference((MemberReferenceHandle)ca.Constructor);
                    switch (member.Parent.Kind)
                    {
                        case HandleKind.TypeReference:
                        {
                            var tr = mr.GetTypeReference((TypeReferenceHandle)member.Parent);
                            return (mr.GetString(tr.Namespace), mr.GetString(tr.Name));
                        }
                        case HandleKind.TypeDefinition:
                        {
                            var td = mr.GetTypeDefinition((TypeDefinitionHandle)member.Parent);
                            return (mr.GetString(td.Namespace), mr.GetString(td.Name));
                        }
                    }
                    break;
                }
            }
        }
        catch (BadImageFormatException)
        {
            // A malformed row: treat the attribute as unrecognised rather than failing the file.
        }
        return ("", "");
    }

    private static CustomAttributeValue<string>? Decode(MetadataReader mr, CustomAttribute ca)
    {
        try
        {
            return ca.DecodeValue(TypeNameProvider.Instance);
        }
        catch (BadImageFormatException) { return null; }
        catch (NotSupportedException) { return null; }
        catch (InvalidOperationException) { return null; }
    }

    /// <summary>
    /// Fold one <c>[HarmonyPatch(…)]</c> into a <see cref="PatchSpec"/>.
    ///
    /// Harmony declares a dozen constructor overloads and they cannot be told apart by arity,
    /// so the arguments are read by their decoded <em>types</em> instead: a
    /// <c>System.Type</c> is the declaring type, a <c>Type[]</c> is the argument-type list, an
    /// integer is a <c>MethodType</c>, and strings are positional — one string is a method
    /// name, two are (typeName, methodName). That covers <c>(Type)</c>, <c>(Type, string)</c>,
    /// <c>(Type, string, Type[])</c>, <c>(Type, MethodType)</c>,
    /// <c>(Type, MethodType, Type[])</c>, <c>(string)</c>, <c>(string, string)</c>,
    /// <c>(string, string, MethodType)</c>, <c>(MethodType)</c>, <c>(MethodType, Type[])</c>
    /// and <c>(Type[])</c>, plus the <c>ArgumentType[]</c> tails, which are variations of the
    /// argument list rather than a target and are ignored.
    /// </summary>
    public static PatchSpec ReadPatch(MetadataReader mr, CustomAttribute ca)
    {
        var spec = new PatchSpec();
        var value = Decode(mr, ca);
        if (value is null) return spec;
        var v = value.Value;

        var strings = new List<string>();
        foreach (var arg in v.FixedArguments)
        {
            switch (arg.Value)
            {
                case string s when TypeNameProvider.Instance.IsSystemType(arg.Type):
                    spec.DeclaringType ??= Names.Clean(s);
                    break;
                case string s:
                    strings.Add(s);
                    break;
                case int i:
                    spec.MethodType ??= i;
                    break;
                case ImmutableArray<CustomAttributeTypedArgument<string>> items:
                {
                    var types = TypeArray(items);
                    if (types is not null) spec.ArgumentTypes ??= types;
                    break;
                }
            }
        }

        if (strings.Count >= 2)
        {
            // (string typeName, string methodName[, MethodType]) — the reflection-only form
            // mods use when the target type is not public.
            spec.DeclaringType ??= Names.Clean(strings[0]);
            spec.MethodName ??= strings[1];
        }
        else if (strings.Count == 1)
        {
            spec.MethodName ??= strings[0];
        }

        foreach (var named in v.NamedArguments)
        {
            switch (named.Name)
            {
                case "declaringType" or "DeclaringType" when named.Value is string dt:
                    spec.DeclaringType = Names.Clean(dt);
                    break;
                case "methodName" or "MethodName" when named.Value is string mn:
                    spec.MethodName = mn;
                    break;
                case "methodType" or "MethodType" when named.Value is int mt:
                    spec.MethodType = mt;
                    break;
                case "argumentTypes" or "ArgumentTypes" when named.Value is ImmutableArray<CustomAttributeTypedArgument<string>> items:
                {
                    var types = TypeArray(items);
                    if (types is not null) spec.ArgumentTypes = types;
                    break;
                }
            }
        }
        return spec;
    }

    /// <summary>
    /// The elements of a <c>Type[]</c> attribute argument, or null when the array is of
    /// something else (<c>ArgumentType[]</c> arrives here too and must not be mistaken for a
    /// signature).
    /// </summary>
    private static List<string>? TypeArray(ImmutableArray<CustomAttributeTypedArgument<string>> items)
    {
        var types = new List<string>(items.Length);
        foreach (var item in items)
        {
            if (item.Value is string s && TypeNameProvider.Instance.IsSystemType(item.Type)) types.Add(Names.Clean(s));
            else if (item.Value is null && TypeNameProvider.Instance.IsSystemType(item.Type)) types.Add("null");
            else return null;
        }
        return types;
    }

    /// <summary>Every string in the attribute's fixed arguments, flattening a <c>params string[]</c>.</summary>
    public static List<string> ReadStrings(MetadataReader mr, CustomAttribute ca)
    {
        var found = new List<string>();
        var value = Decode(mr, ca);
        if (value is null) return found;
        foreach (var arg in value.Value.FixedArguments)
        {
            if (arg.Value is string s) found.Add(s);
            else if (arg.Value is ImmutableArray<CustomAttributeTypedArgument<string>> items)
            {
                foreach (var item in items)
                {
                    if (item.Value is string e) found.Add(e);
                }
            }
        }
        return found;
    }

    /// <summary>The first integer argument — <c>[HarmonyPriority(Priority.First)]</c> and friends.</summary>
    public static int? ReadInt(MetadataReader mr, CustomAttribute ca)
    {
        var value = Decode(mr, ca);
        if (value is null) return null;
        foreach (var arg in value.Value.FixedArguments)
        {
            if (arg.Value is int i) return i;
        }
        return null;
    }

    /// <summary>Harmony's <c>MethodType</c> enum, decoded from the integer in the blob.</summary>
    public static string MethodTypeName(int? value) => value switch
    {
        null or 0 => "normal",
        1 => "getter",
        2 => "setter",
        3 => "constructor",
        4 => "staticConstructor",
        5 => "enumerator",
        6 => "async",
        _ => "normal",
    };
}
