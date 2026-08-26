package io.envy;

import org.yaml.snakeyaml.Yaml;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * Loads and validates {@code envy.yaml} configuration natively inside any JVM
 * application — no CLI required. Mirrors the precedence of the envy binary:
 *
 * <pre>
 * System environment → envy.local.&lt;branch&gt;.yaml → envy.local.yaml → schema default → generated mock
 * </pre>
 *
 * <p>Usage:</p>
 * <pre>{@code
 * Map<String, String> config = Envy.load();
 * String dbUrl = config.get("DATABASE_URL");
 * }</pre>
 *
 * <p>Spring Boot: register it as a bean or call it from a
 * {@code @ConfigurationProperties} initializer.</p>
 */
public final class Envy {

    /** Thrown with every problem collected in a single pass. */
    public static final class EnvyException extends RuntimeException {
        public final List<String> problems;

        EnvyException(List<String> problems) {
            super("envy configuration invalid (" + problems.size() + " problem(s)):\n  - "
                    + String.join("\n  - ", problems));
            this.problems = problems;
        }
    }

    private static final String SCHEMA_FILE = "envy.yaml";
    private static final List<String> KNOWN_SCHEMES = List.of(
            "postgresql", "postgres", "mysql", "mariadb", "mssql", "mongodb",
            "mongodb+srv", "redis", "rediss", "amqp", "rabbitmq", "kafka",
            "http", "https", "ws", "wss", "grpc", "ftp", "sftp", "ssh",
            "smtp", "s3", "gs", "azblob", "sqlite");

    private static volatile Map.Entry<Path, Map<String, String>> cached;

    private Envy() {
    }

    /** Load from the current working directory (searching upward). */
    public static Map<String, String> load() {
        return load(Paths.get("").toAbsolutePath());
    }

    /** Load searching upward from {@code startDir}. Results are cached per process. */
    @SuppressWarnings("unchecked")
    public static Map<String, String> load(Path startDir) {
        Path schemaPath = findUpward(SCHEMA_FILE, startDir.toAbsolutePath());
        if (schemaPath == null) {
            throw new EnvyException(List.of(
                    "no " + SCHEMA_FILE + " found in " + startDir + " or any parent directory"));
        }

        Map<String, String> memo = cached == null ? null : cached.getValue();
        if (memo != null && cached.getKey().equals(schemaPath)) {
            return memo;
        }

        Path baseDir = schemaPath.getParent();

        Yaml yaml = new Yaml();
        Map<String, Object> schemaDoc = readYaml(yaml, schemaPath);
        if (schemaDoc == null) {
            throw new EnvyException(List.of(SCHEMA_FILE + ": empty or invalid schema"));
        }
        Object configObj = schemaDoc.get("config");
        Map<String, Object> config = configObj instanceof Map ? (Map<String, Object>) configObj : Map.of();

        String branch = currentBranch(baseDir);
        Path overlayFile = branch == null ? null
                : baseDir.resolve("envy.local." + sanitizeBranch(branch) + ".yaml");
        Path localFile = baseDir.resolve("envy.local.yaml");

        Map<String, Object> overlay = overlayFile != null && Files.exists(overlayFile)
                ? valuesOf(readYaml(yaml, overlayFile), overlayFile.getFileName().toString())
                : new LinkedHashMap<>();
        Map<String, Object> local = Files.exists(localFile)
                ? valuesOf(readYaml(yaml, localFile), "envy.local.yaml")
                : new LinkedHashMap<>();

        List<String> problems = new ArrayList<>();
        Map<String, String> resolved = new LinkedHashMap<>();
        Set<String> sources = new LinkedHashSet<>();

        for (Map.Entry<String, Object> entry : config.entrySet()) {
            String key = entry.getKey();
            Map<String, Object> spec = entry.getValue() instanceof Map
                    ? (Map<String, Object>) entry.getValue()
                    : new LinkedHashMap<>();

            String placed = null;
            String source = null;

            String envValue = System.getenv(key);
            if (envValue != null) {
                placed = envValue;
                source = "env";
            } else if (overlay.containsKey(key)) {
                placed = scalarToString(overlay.get(key), key, problems);
                source = "overlay";
            } else if (local.containsKey(key)) {
                placed = scalarToString(local.get(key), key, problems);
                source = "local";
            } else if (spec.containsKey("default") && spec.get("default") != null) {
                placed = scalarToString(spec.get("default"), key, problems);
                source = "default";
            } else if (Boolean.TRUE.equals(asBool(spec.get("mock")))) {
                placed = mockValue(key);
                source = "mock";
            }

            if (placed == null) {
                if (Boolean.TRUE.equals(asBool(spec.get("required")))) {
                    problems.add("missing required variable " + key);
                }
                continue;
            }

            checkType(spec, key, placed, problems);
            checkFormat(spec, key, placed, problems);
            resolved.put(key, placed);
            sources.add(source);
        }

        Set<String> known = config.keySet();
        for (Map.Entry<String, Map<String, Object>> layer : List.of(
                Map.entry("branch overlay", overlay),
                Map.entry("envy.local.yaml", local))) {
            for (String key : layer.getValue().keySet()) {
                if (!known.contains(key)) {
                    String suggestion = bestKeyMatch(key, known);
                    problems.add(key + " is set in " + layer.getKey()
                            + " but not declared in " + SCHEMA_FILE + " (typo?)"
                            + (suggestion != null ? " — did you mean " + suggestion + "?" : ""));
                }
            }
        }

        if (!problems.isEmpty()) {
            throw new EnvyException(problems);
        }

        Map<String, String> result = Map.copyOf(resolved);
        cached = Map.entry(schemaPath, result);
        return result;
    }

    /** Convenience accessor over {@link #load()}. */
    public static String get(String key) {
        return load().get(key);
    }

    // ---------- internals ----------

    @SuppressWarnings("unchecked")
    private static Map<String, Object> valuesOf(Map<String, Object> doc, String label) {
        if (doc == null) {
            return new LinkedHashMap<>();
        }
        Object inner = doc.get("values");
        if (inner instanceof Map) {
            return (Map<String, Object>) inner;
        }
        if (!doc.isEmpty()) {
            boolean looksFlat = doc.keySet().stream().allMatch(k -> k.equals(k.toUpperCase()) && !k.contains(":"));
            if (looksFlat && !doc.containsKey("service") && !doc.containsKey("version")) {
                return doc;
            }
        }
        throw new EnvyException(List.of(label + ": expected a `values:` mapping"));
    }

    private static Map<String, Object> readYaml(Yaml yaml, Path file) {
        try {
            String text = Files.readString(file, StandardCharsets.UTF_8);
            if (text.isBlank()) {
                return new LinkedHashMap<>();
            }
            return yaml.load(text);
        } catch (IOException e) {
            throw new EnvyException(List.of("reading " + file.getFileName() + ": " + e.getMessage()));
        } catch (RuntimeException e) {
            throw new EnvyException(List.of("parsing " + file.getFileName() + ": " + e.getMessage()));
        }
    }

    private static Path findUpward(String name, Path start) {
        Path dir = start.normalize();
        for (;;) {
            Path candidate = dir.resolve(name);
            if (Files.isRegularFile(candidate)) {
                return candidate;
            }
            Path parent = dir.getParent();
            if (parent == null || parent.equals(dir)) {
                return null;
            }
            dir = parent;
        }
    }

    private static String currentBranch(Path repoDir) {
        try {
            Process process = new ProcessBuilder(
                    "git", "-C", repoDir.toString(), "rev-parse", "--abbrev-ref", "HEAD")
                    .redirectError(ProcessBuilder.Redirect.DISCARD)
                    .start();
            String out = new String(process.getInputStream().readAllBytes(), StandardCharsets.UTF_8).trim();
            int exit = process.waitFor();
            if (exit == 0 && !out.isEmpty() && !"HEAD".equals(out)) {
                return out;
            }
            return null;
        } catch (IOException | InterruptedException e) {
            return null;
        }
    }

    private static String sanitizeBranch(String branch) {
        StringBuilder sb = new StringBuilder(branch.length());
        for (char c : branch.toCharArray()) {
            sb.append(Character.isLetterOrDigit(c) || c == '.' || c == '_' || c == '-' ? c : '-');
        }
        return sb.toString();
    }

    private static String scalarToString(Object value, String key, List<String> problems) {
        if (value instanceof String s) {
            return s;
        }
        if (value instanceof Boolean || value instanceof Number) {
            return String.valueOf(value);
        }
        problems.add(key + ": expected a scalar value");
        return null;
    }

    private static Boolean asBool(Object value) {
        return value instanceof Boolean b ? b : Boolean.parseBoolean(String.valueOf(value));
    }

    private static void checkType(Map<String, Object> spec, String key, String raw, List<String> problems) {
        String type = String.valueOf(spec.getOrDefault("type", "string"));
        switch (type) {
            case "integer":
                if (!raw.trim().matches("-?\\d+")) {
                    problems.add(key + ": expected an integer, got \"" + raw + "\"");
                }
                break;
            case "number":
            case "float":
                try {
                    Double.parseDouble(raw.trim());
                } catch (NumberFormatException e) {
                    problems.add(key + ": expected a number, got \"" + raw + "\"");
                }
                break;
            case "boolean":
            case "bool":
                if (!List.of("true", "false", "1", "0", "yes", "no", "on", "off")
                        .contains(raw.trim().toLowerCase())) {
                    problems.add(key + ": expected a boolean (true/false), got \"" + raw + "\"");
                }
                break;
            default:
                break;
        }
    }

    private static void checkFormat(Map<String, Object> spec, String key, String raw, List<String> problems) {
        Object formatObj = spec.get("format");
        if (formatObj == null) {
            return;
        }
        String format = String.valueOf(formatObj);
        switch (format) {
            case "uri":
            case "url": {
                int idx = raw.indexOf("://");
                if (idx <= 0 || raw.contains(" ")) {
                    problems.add(key + ": does not satisfy format '" + format + "': \"" + raw + "\"");
                    return;
                }
                String scheme = raw.substring(0, idx);
                if (!KNOWN_SCHEMES.contains(scheme) && nearMissScheme(scheme)) {
                    String suggestion = bestScheme(scheme);
                    if (suggestion != null) {
                        problems.add(key + ": does not satisfy format '" + format
                                + "' — did you mean " + suggestion + "://" + raw.substring(idx + 3) + "?");
                    }
                }
                break;
            }
            case "email": {
                int at = raw.indexOf('@');
                boolean ok = at > 0
                        && raw.indexOf('.', at) > at + 1
                        && !raw.contains(" ");
                if (!ok) {
                    problems.add(key + ": does not satisfy format 'email': \"" + raw + "\"");
                }
                break;
            }
            case "uuid":
                if (!raw.matches("[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")) {
                    problems.add(key + ": does not satisfy format 'uuid': \"" + raw + "\"");
                }
                break;
            default:
                break;
        }
    }

    private static boolean nearMissScheme(String scheme) {
        return bestScheme(scheme) != null;
    }

    private static String bestKeyMatch(String key, Set<String> candidates) {
        String best = null;
        int bestDistance = Integer.MAX_VALUE;
        for (String candidate : candidates) {
            int distance = osaDistance(key, candidate);
            int limit = Math.max(1, Math.min(3, key.length() / 3));
            if (distance <= limit && distance < bestDistance) {
                best = candidate;
                bestDistance = distance;
            }
        }
        return best;
    }

    private static String bestScheme(String scheme) {
        String best = null;
        int bestDistance = Integer.MAX_VALUE;
        for (String candidate : KNOWN_SCHEMES) {
            int distance = osaDistance(scheme, candidate);
            int limit = Math.max(1, Math.min(3, scheme.length() / 3));
            if (distance <= limit && distance < bestDistance) {
                best = candidate;
                bestDistance = distance;
            }
        }
        return best;
    }

    private static String mockValue(String key) {
        StringBuilder hex = new StringBuilder();
        long hash = 0xcbf29ce484222325L;
        for (int i = 0; i < key.length(); i++) {
            hash ^= key.charAt(i);
            hash *= 0x100000001b3L;
        }
        hex.append(String.format("%016x", hash)).append(String.format("%016x", hash >>> 1));
        return "mock_" + hex;
    }

    private static int osaDistance(String a, String b) {
        int[][] d = new int[a.length() + 1][b.length() + 1];
        for (int i = 0; i <= a.length(); i++) {
            d[i][0] = i;
        }
        for (int j = 0; j <= b.length(); j++) {
            d[0][j] = j;
        }
        for (int i = 1; i <= a.length(); i++) {
            for (int j = 1; j <= b.length(); j++) {
                int cost = a.charAt(i - 1) == b.charAt(j - 1) ? 0 : 1;
                d[i][j] = Math.min(Math.min(d[i - 1][j] + 1, d[i][j - 1] + 1), d[i - 1][j - 1] + cost);
                if (i > 1 && j > 1
                        && a.charAt(i - 1) == b.charAt(j - 2)
                        && a.charAt(i - 2) == b.charAt(j - 1)) {
                    d[i][j] = Math.min(d[i][j], d[i - 2][j - 2] + 1);
                }
            }
        }
        return d[a.length()][b.length()];
    }
}
