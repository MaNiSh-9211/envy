# envy-java — Maven / Spring Boot library

Load and validate `envy.yaml` **natively inside any JVM application** — no CLI,
no plugins, no external process. Works with plain Java, Spring Boot, Quarkus,
Micronaut, Android backends… anything on the JVM.

## Add the dependency

```xml
<dependency>
  <groupId>io.github.manish-9211</groupId>
  <artifactId>envy-java</artifactId>
  <version>0.2.0</version>
</dependency>
```

(Once published to Maven Central; until then `mvn install` this module locally.)

## Plain Java

```java
import io.envy.Envy;
import java.util.Map;

Map<String, String> config = Envy.load();
String dbUrl  = config.get("DATABASE_URL");
int port      = Integer.parseInt(config.get("PORT"));
```

## Spring Boot

```java
@Configuration
public class EnvyConfigSource {
    @Bean
    public Map<String, String> envyProperties() {
        return Envy.load();
    }
}

// then anywhere:
@Value("#{envyProperties['DATABASE_URL']}")
private String databaseUrl;
```

Or bridge into Spring's `Environment` with a `MapPropertySource`:

```java
ConfigurableApplicationContext ctx = SpringApplication.run(App.class);
ctx.getEnvironment().getPropertySources()
   .addFirst(new MapPropertySource("envy", Envy.load()));
```

## Behaviour (identical to the Rust core)

- upward search for `envy.yaml` from any working directory
- precedence: OS env → branch overlay (`envy.local.<branch>.yaml`) → `envy.local.yaml` → schema default → generated mock
- validates integer / number / boolean types and `uri` / `email` / `uuid` formats
- typo'd keys and schemes reported with "did you mean …?" suggestions
- throws one `Envy.EnvyException` listing every problem at once
- results cached per schema location (`Envy.load()` is cheap after first call)

## Building & testing

```bash
mvn package    # compiles + runs unit tests
```

## Alternative: wrap the binary instead

Prefer zero JVM dependencies? The envy CLI injects environment variables into
any process, so plain `envy run ./mvnw spring-boot:run` works with zero code
changes — see the repository README. This library exists for teams who want
validation and typing *inside* the JVM.
