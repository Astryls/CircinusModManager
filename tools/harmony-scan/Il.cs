using System.Reflection.Metadata;
using System.Reflection.Metadata.Ecma335;

namespace Circinus.HarmonyScan;

/// <summary>
/// Just enough CIL to walk a method body and read the operands of five instructions.
///
/// A decompiler would do this too, but pulling one in would add a large dependency and a
/// trimming problem to a tool whose whole job is to open a file and print names. Stepping
/// over instructions only needs their operand widths, which is one 256-entry table.
/// </summary>
internal static class Il
{
    public const int Call = 0x28;
    public const int Callvirt = 0x6F;
    public const int Ldstr = 0x72;
    public const int Ldtoken = 0xD0;
    public const int Newobj = 0x73;

    private static readonly sbyte[] Size1 = BuildSize1();
    private static readonly sbyte[] Size2 = BuildSize2();

    private const sbyte Invalid = -1;
    private const sbyte Switch = -2;

    private static sbyte[] BuildSize1()
    {
        var t = new sbyte[256];
        void Set(int from, int to, sbyte size)
        {
            for (var i = from; i <= to; i++) t[i] = size;
        }

        // Short-form inline operands: one byte.
        Set(0x0E, 0x13, 1);
        t[0x1F] = 1;               // ldc.i4.s
        Set(0x2B, 0x37, 1);        // br.s … blt.un.s
        t[0xDE] = 1;               // leave.s

        // Four-byte operands: tokens, int32 constants, long branch targets.
        t[0x20] = 4;               // ldc.i4
        t[0x22] = 4;               // ldc.r4
        Set(0x27, 0x29, 4);        // jmp, call, calli
        Set(0x38, 0x44, 4);        // br … blt.un
        Set(0x6F, 0x75, 4);        // callvirt, cpobj, ldobj, ldstr, newobj, castclass, isinst
        t[0x79] = 4;               // unbox
        Set(0x7B, 0x81, 4);        // ldfld … stobj
        Set(0x8C, 0x8D, 4);        // box, newarr
        t[0x8F] = 4;               // ldelema
        Set(0xA3, 0xA5, 4);        // ldelem, stelem, unbox.any
        t[0xC2] = 4;               // refanyval
        t[0xC6] = 4;               // mkrefany
        t[0xD0] = 4;               // ldtoken
        t[0xDD] = 4;               // leave

        // Eight-byte constants.
        t[0x21] = 8;               // ldc.i8
        t[0x23] = 8;               // ldc.r8

        t[0x45] = Switch;

        // Opcodes ECMA-335 leaves unused: hitting one means we have lost the instruction
        // stream, so the walk stops rather than reporting nonsense.
        t[0x24] = Invalid;
        Set(0x77, 0x78, Invalid);
        Set(0xA6, 0xB2, Invalid);
        Set(0xBB, 0xC1, Invalid);
        Set(0xC4, 0xC5, Invalid);
        Set(0xC7, 0xCF, Invalid);
        Set(0xE1, 0xFD, Invalid);
        t[0xFF] = Invalid;
        return t;
    }

    private static sbyte[] BuildSize2()
    {
        var t = new sbyte[256];
        for (var i = 0; i < 256; i++) t[i] = Invalid;
        for (var i = 0x00; i <= 0x1E; i++) t[i] = 0;
        t[0x06] = 4;               // ldftn
        t[0x07] = 4;               // ldvirtftn
        t[0x08] = Invalid;
        for (var i = 0x09; i <= 0x0E; i++) t[i] = 2;   // ldarg … stloc (long form)
        t[0x10] = Invalid;
        t[0x12] = 1;               // unaligned.
        t[0x15] = 4;               // initobj
        t[0x16] = 4;               // constrained.
        t[0x19] = 1;               // no.
        t[0x1C] = 4;               // sizeof
        return t;
    }

    /// <summary>
    /// Walk <paramref name="il"/>, yielding (opcode, operand) for the instructions that carry a
    /// four-byte operand and 0 for the rest. Two-byte opcodes are reported as
    /// <c>0x100 | second</c>; nothing here needs them, but they must still be stepped over.
    /// The walk stops silently at the first byte it cannot interpret.
    /// </summary>
    public static IEnumerable<(int Op, int Operand)> Read(byte[] il)
    {
        var i = 0;
        while (i < il.Length)
        {
            int op = il[i++];
            sbyte size;
            if (op == 0xFE)
            {
                if (i >= il.Length) yield break;
                int second = il[i++];
                size = Size2[second];
                op = 0x100 | second;
            }
            else
            {
                size = Size1[op];
            }

            if (size == Invalid) yield break;
            if (size == Switch)
            {
                if (i + 4 > il.Length) yield break;
                var count = BitConverter.ToUInt32(il, i);
                i += 4;
                // A bogus count would run the index off the end; bail instead of overflowing.
                if (count > (uint)((il.Length - i) / 4)) yield break;
                i += (int)count * 4;
                continue;
            }

            if (i + size > il.Length) yield break;
            var operand = size == 4 ? BitConverter.ToInt32(il, i) : 0;
            i += size;
            yield return (op, operand);
        }
    }

    /// <summary>The user string a <c>ldstr</c> token points at, or null if the token is bad.</summary>
    public static string? UserString(MetadataReader mr, int token)
    {
        try
        {
            var handle = MetadataTokens.Handle(token);
            return handle.Kind == HandleKind.UserString ? mr.GetUserString((UserStringHandle)handle) : null;
        }
        catch (Exception e) when (e is BadImageFormatException or ArgumentException)
        {
            return null;
        }
    }

    /// <summary>Declaring type and member name behind a call/newobj token, or null.</summary>
    public static (string Type, string Name)? Member(MetadataReader mr, int token)
    {
        try
        {
            var handle = MetadataTokens.EntityHandle(token);
            switch (handle.Kind)
            {
                case HandleKind.MemberReference:
                {
                    var member = mr.GetMemberReference((MemberReferenceHandle)handle);
                    var type = Names.TypeOf(mr, member.Parent);
                    return type is null ? null : (type, mr.GetString(member.Name));
                }
                case HandleKind.MethodDefinition:
                {
                    var md = mr.GetMethodDefinition((MethodDefinitionHandle)handle);
                    return (Names.FullName(mr, md.GetDeclaringType()), mr.GetString(md.Name));
                }
                case HandleKind.MethodSpecification:
                {
                    var spec = mr.GetMethodSpecification((MethodSpecificationHandle)handle);
                    return Member(mr, MetadataTokens.GetToken(spec.Method));
                }
                default:
                    return null;
            }
        }
        catch (Exception e) when (e is BadImageFormatException or ArgumentException or InvalidOperationException)
        {
            return null;
        }
    }

    /// <summary>The type a <c>ldtoken</c> pushes, or null when it pushes a method or field instead.</summary>
    public static string? TokenType(MetadataReader mr, int token)
    {
        try
        {
            return Names.TypeOf(mr, MetadataTokens.EntityHandle(token));
        }
        catch (Exception e) when (e is BadImageFormatException or ArgumentException)
        {
            return null;
        }
    }
}
