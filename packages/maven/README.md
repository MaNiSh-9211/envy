# envy for Java / Spring Boot (Maven)

Java devs never need an npm/pip package. envy injects plain environment
variables, so the JVM reads them natively via `System.getenv("PORT")` or
Spring's `${PORT}` placeholder.

## One-time setup per project

Add these two plugins to your `pom.xml`. The first downloads the platform
binary during the `initialize` phase; the second lets you run anything through
envy with `mvn envy` style profiles.

```xml
<plugin>
  <groupId>com.googlecode.maven-download-plugin</groupId>
  <artifactId>download-maven-plugin</artifactId>
  <version>1.13.0</version>
  <executions>
    <execution>
      <id>fetch-envy</id>
      <phase>initialize</phase>
      <goals>
        <goal>wget</goal>
      </goals>
      <configuration>
        <url>https://github.com/MaNiSh-9211/envy/releases/latest/download/${envy.asset}</url>
        <outputDirectory>${project.build.directory}/envy</outputDirectory>
        <outputFilename>envy${envy.ext}</outputFilename>
      </configuration>
    </execution>
  </executions>
</plugin>

<plugin>
  <groupId>org.codehaus.mojo</groupId>
  <artifactId>exec-maven-plugin</artifactId>
  <version>3.5.0</version>
  <configuration>
    <executable>${project.build.directory}/envy/envy${envy.ext}</executable>
  </configuration>
</plugin>
```

And in `<properties>` pick your platform asset:

```xml
<properties>
  <!-- windows -->
  <envy.asset>envy-windows-amd64.exe</envy.asset>
  <envy.ext>.exe</envy.ext>
  <!-- mac arm: envy-darwin-arm64 | mac intel: envy-darwin-amd64 -->
  <!-- linux amd64: envy-linux-amd64 | linux arm: envy-linux-arm64 -->
</properties>
```

## Daily usage

```bash
mvn initialize                                   # fetch binary once
envy run ./mvnw spring-boot:run                  # recommended: wrap maven itself
```

or purely inside Maven:

```bash
mvn initialize exec:exec -Dexec.args="run ./mvnw spring-boot:run"
```

Your application reads values exactly as before:

```java
@Value("${PORT}")
private int port;   // supplied by envy at boot
```
