package main

import (
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"runtime"
)

const repo = "MaNiSh-9211/envy"

func main() {
	osName := map[string]string{"windows": "windows", "darwin": "darwin"}[runtime.GOOS]
	if osName == "" {
		osName = "linux"
	}
	cpu := runtime.GOARCH
	ext := ""
	if osName == "windows" {
		ext = ".exe"
	}
	asset := fmt.Sprintf("envy-%s-%s%s", osName, cpu, ext)

	version := os.Getenv("ENVY_VERSION")
	if version == "" || version == "latest" {
		version = "latest/download"
	} else {
		version = "download/" + version
	}
	url := fmt.Sprintf("https://github.com/%s/releases/%s/%s", repo, version, asset)

	fmt.Println("envy: downloading", url)
	resp, err := http.Get(url)
	if err != nil {
		fatal(err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		fatal(fmt.Errorf("HTTP %d fetching %s", resp.StatusCode, url))
	}

	binDir := os.Getenv("GOBIN")
	if binDir == "" {
		home, homeErr := os.UserHomeDir()
		if homeErr != nil {
			fatal(homeErr)
		}
		binDir = filepath.Join(home, "go", "bin")
	}
	if mkErr := os.MkdirAll(binDir, 0o755); mkErr != nil {
		fatal(mkErr)
	}

	outPath := filepath.Join(binDir, asset)
	file, createErr := os.OpenFile(outPath, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0o755)
	if createErr != nil {
		fatal(createErr)
	}
	if _, copyErr := io.Copy(file, resp.Body); copyErr != nil {
		file.Close()
		fatal(copyErr)
	}
	if closeErr := file.Close(); closeErr != nil {
		fatal(closeErr)
	}
	_ = os.Chmod(outPath, 0o755)

	fmt.Println("envy installed:", outPath)
	fmt.Println("tip: rename/link it to `envy` or ensure", binDir, "is on your PATH")
}

func fatal(err error) {
	fmt.Fprintln(os.Stderr, "envy:", err)
	os.Exit(1)
}
