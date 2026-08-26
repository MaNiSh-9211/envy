using System.Collections.Concurrent;
using System.Diagnostics;
using System.Text;
using YamlDotNet.Serialization;

namespace Envy;

/// <summary>
/// Loads and validates <c>envy.yaml</c> natively inside any .NET application —
/// ASP.NET Core, Worker Services, console apps. Mirrors the precedence of the
/// envy core engine:
///
///   Environment variables → envy.local.&lt;branch&gt;.yaml → envy.local.yaml → schema default → generated mock
///
/// Usage:
/// <code>
///   var config = EnvyLoader.Load();
///   string dbUrl = config["DATABASE_URL"];
/// </code>
/// </summary>
public static class EnvyLoader
{
    private const string SchemaFile = "envy.yaml";

    private static readonly string[] KnownSchemes =
    {
        "postgresql", "postgres", "mysql", "mariadb", "mssql", "mongodb",
        "mongodb+srv", "redis", "rediss", "amqp", "rabbitmq", "kafka",
        "http", "https", "ws", "wss", "grpc", "ftp", "sftp", "ssh",
        "smtp", "s3", "gs", "azblob", "sqlite",
    };

    private static readonly ConcurrentDictionary<string, IReadOnlyDictionary<string, string>> Cache =
        new(StringComparer.Ordinal);

    public sealed class EnvyException : Exception
    {
        public IReadOnlyList<string> Problems { get; }

        internal EnvyException(IReadOnlyList<string> problems)
            : base($"envy configuration invalid ({problems.Count} problem(s)):\n  - " + string.Join("\n  - ", problems))
        {
            Problems = problems;
        }
    }

    /// <summary>Load from the current working directory (searching upward).</summary>
    public static IReadOnlyDictionary<string, string> Load()
    {
        return Load(Directory.GetCurrentDirectory());
    }

    /// <summary>Load searching upward from <paramref name="startDir"/>. Cached per schema location.</summary>
    public static IReadOnlyDictionary<string, string> Load(string startDir)
    {
        var schemaPath = FindUpward(SchemaFile, Path.GetFullPath(startDir));
        if (schemaPath == null)
        {
            throw new EnvyException(new[]
            {
                $"no {SchemaFile} found in {startDir} or any parent directory — run `envy init` first",
            });
        }

        return Cache.GetOrAdd(schemaPath, _ => Resolve(schemaPath));
    }

    /// <summary>Convenience accessor over <see cref="Load()"/>.</summary>
    public static string? Get(string key)
    {
        return Load().TryGetValue(key, out var value) ? value : null;
    }

    // ---------------------------------------------------------------- internals

    private static IReadOnlyDictionary<string, string> Resolve(string schemaPath)
    {
        var problems = new List<string>();
        var baseDir = Path.GetDirectoryName(schemaPath) ?? ".";

        Dictionary<object, object>? schemaDoc = ReadYaml(schemaPath, problems);
        if (schemaDoc == null)
        {
            throw new EnvyException(new[] { $"{SchemaFile}: empty or invalid schema" });
        }

        var config = GetMap(GetEntry(schemaDoc, "config"));

        string? branch = CurrentBranch(baseDir);
        var overlayFile = branch != null
            ? Path.Combine(baseDir, $"envy.local.{SanitizeBranch(branch)}.yaml")
            : null;
        var localFile = Path.Combine(baseDir, "envy.local.yaml");

        var overlay = overlayFile != null && File.Exists(overlayFile)
            ? ValuesOf(ReadYaml(overlayFile, problems), Path.GetFileName(overlayFile), problems)
            : new Dictionary<string, object>();
        var local = File.Exists(localFile)
            ? ValuesOf(ReadYaml(localFile, problems), "envy.local.yaml", problems)
            : new Dictionary<string, object>();

        var resolved = new Dictionary<string, string>(StringComparer.Ordinal);

        foreach (var pair in config)
        {
            var key = pair.Key;
            var spec = GetMap(pair.Value);

            string? value = null;
            bool placed = false;

            var envValue = Environment.GetEnvironmentVariable(key);
            if (!string.IsNullOrEmpty(envValue))
            {
                value = envValue;
                placed = true;
            }
            else if (overlay.TryGetValue(key, out var overlayValue))
            {
                value = ScalarToString(overlayValue, key, problems);
                placed = value != null;
            }
            else if (local.TryGetValue(key, out var localValue))
            {
                value = ScalarToString(localValue, key, problems);
                placed = value != null;
            }
            else if (GetEntry(spec, "default") is { } defaultValue && defaultValue != null)
            {
                value = ScalarToString(defaultValue, key, problems);
                placed = value != null;
            }
            else if (IsTrue(GetEntry(spec, "mock")))
            {
                value = MockValue(key);
                placed = true;
            }

            if (!placed || value == null)
            {
                if (IsTrue(GetEntry(spec, "required")))
                {
                    problems.Add($"missing required variable {key}");
                }
                continue;
            }

            CheckType(spec, key, value, problems);
            CheckFormat(spec, key, value, problems);
            resolved[key] = value;
        }

        foreach (var layer in new[] { ("branch overlay", (IDictionary<string, object>)overlay), ("envy.local.yaml", (IDictionary<string, object>)local) })
        {
            foreach (var key in layer.Item2.Keys)
            {
                if (!config.ContainsKey(key))
                {
                    var suggestion = BestKeyMatch(key, config.Keys);
                    problems.Add(
                        $"{key} is set in {layer.Item1} but not declared in {SchemaFile} (typo?)"
                        + (suggestion != null ? $" — did you mean {suggestion}?" : string.Empty));
                }
            }
        }

        if (problems.Count > 0)
        {
            throw new EnvyException(problems);
        }

        return new Dictionary<string, string>(resolved, StringComparer.Ordinal);
    }

    private static object? GetEntry(Dictionary<object, object> map, string key)
    {
        foreach (var pair in map)
        {
            if (string.Equals(pair.Key as string ?? pair.Key?.ToString(), key, StringComparison.Ordinal))
            {
                return pair.Value;
            }
        }
        return null;
    }

    private static Dictionary<object, object> GetMap(object? value)
    {
        if (value is IDictionary<object, object> generic)
        {
            return new Dictionary<object, object>(generic);
        }
        if (value is IDictionary<object, object?> nullableGeneric)
        {
            var result = new Dictionary<object, object>();
            foreach (var pair in nullableGeneric)
            {
                if (pair.Value != null)
                {
                    result[pair.Key] = pair.Value!;
                }
            }
            return result;
        }
        return new Dictionary<object, object>();
    }

    private static Dictionary<string, object> ValuesOf(Dictionary<object, object>? doc, string label, List<string> problems)
    {
        if (doc == null || doc.Count == 0)
        {
            return new Dictionary<string, object>();
        }
        if (GetEntry(doc, "values") is { } valuesNode)
        {
            var inner = GetMap(valuesNode);
            var typed = new Dictionary<string, object>();
            foreach (var pair in inner)
            {
                var key = pair.Key?.ToString();
                if (!string.IsNullOrEmpty(key))
                {
                    typed[key!] = pair.Value;
                }
            }
            return typed;
        }

        var flat = new Dictionary<string, object>();
        var looksFlat = true;
        foreach (var pair in doc)
        {
            var key = pair.Key?.ToString() ?? string.Empty;
            if (key == "service" || key == "version" || key != key.ToUpperInvariant())
            {
                looksFlat = false;
                break;
            }
            flat[key] = pair.Value;
        }
        if (looksFlat && flat.Count > 0)
        {
            return flat;
        }

        problems.Add($"{label}: expected a `values:` mapping or flat KEY: value pairs");
        return new Dictionary<string, object>();
    }

    private static Dictionary<object, object>? ReadYaml(string file, List<string> problems)
    {
        try
        {
            var text = File.ReadAllText(file);
            if (string.IsNullOrWhiteSpace(text))
            {
                return new Dictionary<object, object>();
            }
            var deserializer = new DeserializerBuilder().Build();
            return deserializer.Deserialize<Dictionary<object, object>>(text);
        }
        catch (Exception ex)
        {
            problems.Add($"{Path.GetFileName(file)}: {ex.Message}");
            return null;
        }
    }

    private static string? ScalarToString(object? value, string key, List<string> problems)
    {
        switch (value)
        {
            case null:
                return null;
            case string s:
                return s;
            case bool b:
                return b ? "true" : "false";
            case IFormattable formattable when !(value is DateTime):
                return formattable.ToString(null, System.Globalization.CultureInfo.InvariantCulture);
            default:
                problems.Add($"{key}: expected a scalar value");
                return null;
        }
    }

    private static bool IsTrue(object? value)
    {
        return value is bool b && b;
    }

    private static string? CurrentBranch(string repoDir)
    {
        try
        {
            using var process = Process.Start(new ProcessStartInfo
            {
                FileName = "git",
                Arguments = $"-C \"{repoDir}\" rev-parse --abbrev-ref HEAD",
                UseShellExecute = false,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                CreateNoWindow = true,
            });
            if (process == null)
            {
                return null;
            }
            var output = process.StandardOutput.ReadToEnd().Trim();
            process.WaitForExit(2000);
            if (process.ExitCode != 0 || output.Length == 0 || output == "HEAD")
            {
                return null;
            }
            return output;
        }
        catch
        {
            return null;
        }
    }

    private static string SanitizeBranch(string branch)
    {
        var sb = new StringBuilder(branch.Length);
        foreach (var c in branch)
        {
            sb.Append(char.IsLetterOrDigit(c) || c == '.' || c == '_' || c == '-' ? c : '-');
        }
        return sb.ToString();
    }

    private static void CheckType(Dictionary<object, object> spec, string key, string raw, List<string> problems)
    {
        var type = GetEntry(spec, "type") as string ?? "string";
        var trimmed = raw.Trim();
        switch (type)
        {
            case "integer":
                if (!long.TryParse(trimmed, out _))
                {
                    problems.Add($"{key}: expected an integer, got \"{raw}\"");
                }
                break;
            case "number":
            case "float":
                if (!double.TryParse(trimmed, System.Globalization.NumberStyles.Float,
                        System.Globalization.CultureInfo.InvariantCulture, out _))
                {
                    problems.Add($"{key}: expected a number, got \"{raw}\"");
                }
                break;
            case "boolean":
            case "bool":
                switch (trimmed.ToLowerInvariant())
                {
                    case "true":
                    case "false":
                    case "1":
                    case "0":
                    case "yes":
                    case "no":
                    case "on":
                    case "off":
                        break;
                    default:
                        problems.Add($"{key}: expected a boolean (true/false), got \"{raw}\"");
                        break;
                }
                break;
        }
    }

    private static void CheckFormat(Dictionary<object, object> spec, string key, string raw, List<string> problems)
    {
        if (GetEntry(spec, "format") is not string format)
        {
            return;
        }
        switch (format)
        {
            case "uri":
            case "url":
            {
                var idx = raw.IndexOf("://", StringComparison.Ordinal);
                if (idx <= 0 || ContainsWhitespace(raw))
                {
                    problems.Add($"{key}: does not satisfy format '{format}': \"{raw}\"");
                    break;
                }
                var scheme = raw.Substring(0, idx);
                if (!KnownSchemes.Contains(scheme))
                {
                    var best = BestScheme(scheme);
                    if (best != null)
                    {
                        problems.Add($"{key}: does not satisfy format '{format}' — did you mean {best}://{raw.Substring(idx + 3)}?");
                    }
                }
                break;
            }
            case "email":
            {
                var at = raw.IndexOf('@');
                var dotAfter = at >= 0 ? raw.IndexOf('.', at + 1) : -1;
                if (at <= 0 || dotAfter <= at + 1 || ContainsWhitespace(raw))
                {
                    problems.Add($"{key}: does not satisfy format 'email': \"{raw}\"");
                }
                break;
            }
            case "uuid":
            {
                if (!System.Text.RegularExpressions.Regex.IsMatch(
                        raw, @"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$"))
                {
                    problems.Add($"{key}: does not satisfy format 'uuid': \"{raw}\"");
                }
                break;
            }
        }
    }

    private static bool ContainsWhitespace(string raw)
    {
        foreach (var c in raw)
        {
            if (char.IsWhiteSpace(c))
            {
                return true;
            }
        }
        return false;
    }

    private static string? BestKeyMatch(string key, ICollection<string> candidates)
    {
        var limit = Math.Max(1, Math.Min(3, key.Length / 3));
        string? best = null;
        var bestDistance = int.MaxValue;
        foreach (var candidate in candidates)
        {
            var distance = OsaDistance(key, candidate);
            if (distance <= limit && distance < bestDistance)
            {
                best = candidate;
                bestDistance = distance;
            }
        }
        return best;
    }

    private static string? BestScheme(string scheme)
    {
        var limit = Math.Max(1, Math.Min(3, scheme.Length / 3));
        string? best = null;
        var bestDistance = int.MaxValue;
        foreach (var candidate in KnownSchemes)
        {
            var distance = OsaDistance(scheme, candidate);
            if (distance <= limit && distance < bestDistance)
            {
                best = candidate;
                bestDistance = distance;
            }
        }
        return best;
    }

    private static string MockValue(string key)
    {
        unchecked
        {
            ulong hash = 0xcbf29ce484222325UL;
            foreach (var c in key)
            {
                hash ^= c;
                hash *= 0x100000001b3UL;
            }
            return $"mock_{hash:x16}{(hash >> 1):x16}";
        }
    }

    private static int OsaDistance(string a, string b)
    {
        var d = new int[a.Length + 1, b.Length + 1];
        for (var i = 0; i <= a.Length; i++)
        {
            d[i, 0] = i;
        }
        for (var j = 0; j <= b.Length; j++)
        {
            d[0, j] = j;
        }
        for (var i = 1; i <= a.Length; i++)
        {
            for (var j = 1; j <= b.Length; j++)
            {
                var cost = a[i - 1] == b[j - 1] ? 0 : 1;
                d[i, j] = Math.Min(Math.Min(d[i - 1, j] + 1, d[i, j - 1] + 1), d[i - 1, j - 1] + cost);
                if (i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1])
                {
                    d[i, j] = Math.Min(d[i, j], d[i - 2, j - 2] + 1);
                }
            }
        }
        return d[a.Length, b.Length];
    }
}
