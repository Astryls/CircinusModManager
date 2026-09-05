using System.Text.Json;

namespace Circinus.HarmonyScan;

/// <summary>
/// harmony-scan — says what a mod's assemblies patch, without running them.
///
/// Mod assemblies target a runtime this tool is not, and loading somebody's code to find out
/// what it does is not a thing a mod manager should do. So nothing here loads an assembly: it
/// reads metadata and IL with System.Reflection.Metadata and reports what it finds.
///
///   harmony-scan &lt;path&gt;...      a .dll, or a folder to search for .dll files
///   --pretty                     indent the JSON
///
/// The JSON goes to stdout, diagnostics to stderr, and the exit code is 0 whenever the scan
/// ran — a file that could not be read is reported in its own entry's `error`, because one bad
/// assembly should not cost the caller the other two hundred.
/// </summary>
internal static class Program
{
    private const int Version = 1;

    private static int Main(string[] args)
    {
        var paths = new List<string>();
        var pretty = false;
        foreach (var a in args)
        {
            switch (a)
            {
                case "--pretty":
                    pretty = true;
                    break;
                case "--json":
                    break;
                case "-h":
                case "--help":
                    Console.Error.WriteLine("usage: harmony-scan [--pretty] <assembly-or-folder>...");
                    return 0;
                default:
                    if (a.StartsWith('-'))
                    {
                        Console.Error.WriteLine($"harmony-scan: unknown option {a}");
                        return 2;
                    }
                    paths.Add(a);
                    break;
            }
        }
        if (paths.Count == 0)
        {
            Console.Error.WriteLine("usage: harmony-scan [--pretty] <assembly-or-folder>...");
            return 2;
        }

        var files = new List<string>();
        foreach (var p in paths)
        {
            try
            {
                if (Directory.Exists(p))
                {
                    files.AddRange(Directory.EnumerateFiles(p, "*.dll", SearchOption.AllDirectories));
                }
                else if (File.Exists(p))
                {
                    files.Add(p);
                }
                else
                {
                    Console.Error.WriteLine($"harmony-scan: {p} does not exist");
                }
            }
            catch (Exception e)
            {
                Console.Error.WriteLine($"harmony-scan: {p}: {e.Message}");
            }
        }
        // Stable output: the caller caches by path, and a diff between two runs should be about
        // the mods, not about the order the file system happened to hand them over in.
        files.Sort(StringComparer.OrdinalIgnoreCase);

        var results = new List<AssemblyResult>(files.Count);
        foreach (var f in files)
        {
            try
            {
                results.Add(Scanner.Scan(f));
            }
            catch (Exception e)
            {
                results.Add(new AssemblyResult { Path = f, Error = e.Message });
            }
        }

        using var stdout = Console.OpenStandardOutput();
        using var w = new Utf8JsonWriter(stdout, new JsonWriterOptions { Indented = pretty });
        Write(w, results);
        w.Flush();
        return 0;
    }

    /// <summary>
    /// The JSON is written by hand rather than by a serializer: NativeAOT has no reflection to
    /// fall back on, and the shape is a contract with the Rust side, so it is worth seeing.
    /// </summary>
    private static void Write(Utf8JsonWriter w, List<AssemblyResult> results)
    {
        w.WriteStartObject();
        w.WriteNumber("version", Version);
        w.WriteStartArray("assemblies");
        foreach (var r in results)
        {
            w.WriteStartObject();
            w.WriteString("path", r.Path);
            WriteOrNull(w, "name", r.Name);
            WriteOrNull(w, "mvid", r.Mvid);
            WriteOrNull(w, "error", r.Error);
            WriteStrings(w, "harmonyIds", r.HarmonyIds);

            w.WriteStartArray("patches");
            foreach (var p in r.Patches)
            {
                w.WriteStartObject();
                w.WriteString("declaringType", p.DeclaringType);
                w.WriteString("method", p.Method);
                w.WriteString("kind", p.Kind);
                WriteOrNull(w, "targetType", p.TargetType);
                WriteOrNull(w, "targetMethod", p.TargetMethod);
                w.WriteString("targetKind", p.TargetKind);
                if (p.ArgumentTypes is null)
                {
                    w.WriteNull("argumentTypes");
                }
                else
                {
                    WriteStrings(w, "argumentTypes", p.ArgumentTypes);
                }
                if (p.Priority is int prio)
                {
                    w.WriteNumber("priority", prio);
                }
                else
                {
                    w.WriteNull("priority");
                }
                WriteStrings(w, "before", p.Before);
                WriteStrings(w, "after", p.After);
                w.WriteString("source", p.Source);
                w.WriteEndObject();
            }
            w.WriteEndArray();

            w.WriteStartArray("manualPatches");
            foreach (var m in r.ManualPatches)
            {
                w.WriteStartObject();
                w.WriteString("declaringType", m.DeclaringType);
                w.WriteString("method", m.Method);
                w.WriteString("detail", m.Detail);
                w.WriteEndObject();
            }
            w.WriteEndArray();

            WriteStrings(w, "startupClasses", r.StartupClasses);
            WriteStrings(w, "modClasses", r.ModClasses);
            w.WriteEndObject();
        }
        w.WriteEndArray();
        w.WriteEndObject();
    }

    private static void WriteOrNull(Utf8JsonWriter w, string name, string? value)
    {
        if (value is null)
        {
            w.WriteNull(name);
        }
        else
        {
            w.WriteString(name, value);
        }
    }

    private static void WriteStrings(Utf8JsonWriter w, string name, List<string> values)
    {
        w.WriteStartArray(name);
        foreach (var v in values)
        {
            w.WriteStringValue(v);
        }
        w.WriteEndArray();
    }
}
