package io.envy;

import org.junit.Test;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Map;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertThrows;
import static org.junit.Assert.assertTrue;

public class EnvyTest {

    private static Path writeFixture(String schemaYaml, String localYaml) throws IOException {
        Path dir = Files.createTempDirectory("envy-test");
        Files.writeString(dir.resolve("envy.yaml"), schemaYaml, StandardCharsets.UTF_8);
        if (localYaml != null) {
            Files.writeString(dir.resolve("envy.local.yaml"), localYaml, StandardCharsets.UTF_8);
        }
        return dir;
    }

    @Test
    public void layersPrecedenceDefaultsAndMocks() throws Exception {
        Path fixture = writeFixture(
                """
                version: "1"
                service: test-svc
                config:
                  PORT:
                    type: integer
                    default: 8080
                  DATABASE_URL:
                    type: string
                    format: uri
                    required: true
                  THIRD_PARTY_TOKEN:
                    type: string
                    mock: true
                """,
                """
                values:
                  DATABASE_URL: "postgresql://u:p@localhost:5432/db"
                """);

        Map<String, String> config = Envy.load(fixture);

        assertEquals("postgresql://u:p@localhost:5432/db", config.get("DATABASE_URL"));
        assertEquals("8080", config.get("PORT"));
        assertTrue(config.get("THIRD_PARTY_TOKEN").startsWith("mock_"));
    }

    @Test
    public void collectsAllProblemsWithSuggestions() throws Exception {
        Path fixture = writeFixture(
                """
                version: "1"
                service: test-svc
                config:
                  DATABASE_URL:
                    type: string
                    format: uri
                    required: true
                """,
                """
                values:
                  DATABASEURL: "typo key"
                  DATABASE_URL: "postgersql://x"
                """);

        Envy.EnvyException error = assertThrows(Envy.EnvyException.class, () -> Envy.load(fixture));
        assertTrue(error.problems.stream().anyMatch(p -> p.contains("did you mean DATABASE_URL?")));
        assertTrue(error.problems.stream().anyMatch(p -> p.contains("did you mean postgresql://")));
    }

    @Test
    public void missingRequiredFailsFast() throws Exception {
        Path fixture = writeFixture(
                """
                version: "1"
                service: test-svc
                config:
                  API_SECRET:
                    type: string
                    secret: true
                    required: true
                """,
                null);

        assertThrows(Envy.EnvyException.class, () -> Envy.load(fixture));
    }
}
