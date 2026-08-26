import os
import platform
import stat
import subprocess
import sys
import urllib.request
from pathlib import Path

REPO = "MaNiSh-9211/envy"


def _asset() -> str:
    system_map = {"Windows": "windows", "Darwin": "darwin"}
    os_name = system_map.get(platform.system(), "linux")
    machine = platform.machine().lower()
    cpu = "arm64" if machine in ("arm64", "aarch64") else "amd64"
    ext = ".exe" if os_name == "windows" else ""
    return f"envy-{os_name}-{cpu}{ext}"


def _binary_path() -> Path:
    base = Path(os.environ.get("LOCALAPPDATA") or Path.home() / ".cache")
    cache_dir = base / "envy" / "bin"
    cache_dir.mkdir(parents=True, exist_ok=True)
    return cache_dir / _asset()


def _download() -> None:
    version = os.environ.get("ENVY_VERSION", "latest")
    segment = "latest/download" if version == "latest" else f"download/{version}"
    url = f"https://github.com/{REPO}/releases/{segment}/{_asset()}"
    print(f"envy: downloading {url}")
    request = urllib.request.Request(url, headers={"User-Agent": "envy-installer"})
    with urllib.request.urlopen(request) as response:
        data = response.read()
    exe = _binary_path()
    exe.write_bytes(data)
    if os.name != "nt":
        exe.chmod(exe.stat().st_mode | stat.S_IEXEC)


def main() -> None:
    exe = _binary_path()
    if not exe.exists():
        try:
            _download()
        except Exception as err:
            print(f"envy: download failed ({err})", file=sys.stderr)
            sys.exit(1)
    try:
        sys.exit(subprocess.call([str(exe), *sys.argv[1:]]))
    except KeyboardInterrupt:
        sys.exit(130)


if __name__ == "__main__":
    main()
