"""Install a pinned official Tinymist binary for CI, verifying its SHA-256."""
import hashlib
import io
import os
from pathlib import Path, PurePosixPath
import platform
import re
import sys
import tarfile
from urllib.request import urlopen
import zipfile


def install(version, destination):
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise ValueError("Expected a fixed release version, e.g. 0.15.8")
    architecture = {"arm64": "aarch64", "aarch64": "aarch64",
                    "amd64": "x86_64", "x86_64": "x86_64"}[platform.machine().lower()]
    target = {"Darwin": "apple-darwin", "Linux": "unknown-linux-gnu",
              "Windows": "pc-windows-msvc"}[platform.system()]
    windows = platform.system() == "Windows"
    archive = f"tinymist-{architecture}-{target}.{'zip' if windows else 'tar.gz'}"
    base = f"https://github.com/Myriad-Dreamin/tinymist/releases/download/v{version}/{archive}"
    with urlopen(base, timeout=60) as response:
        data = response.read()
    with urlopen(base + ".sha256", timeout=30) as response:
        expected = response.read().decode("utf-8").split()[0]
    if hashlib.sha256(data).hexdigest() != expected:
        raise ValueError(f"Checksum mismatch for {archive}")
    executable = "tinymist.exe" if windows else "tinymist"
    # Read only the executable, without extracting arbitrary archive paths.
    if windows:
        with zipfile.ZipFile(io.BytesIO(data)) as bundle:
            matches = [name for name in bundle.namelist() if PurePosixPath(name).name == executable]
            if len(matches) != 1:
                raise ValueError(f"Expected one {executable} in {archive}")
            binary = bundle.read(matches[0])
    else:
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as bundle:
            matches = [member for member in bundle.getmembers()
                       if member.isfile() and PurePosixPath(member.name).name == executable]
            if len(matches) != 1:
                raise ValueError(f"Expected one {executable} in {archive}")
            with bundle.extractfile(matches[0]) as source:
                binary = source.read()
    destination.mkdir(parents=True, exist_ok=True)
    path = destination / executable
    path.write_bytes(binary)
    path.chmod(0o755)
    if "GITHUB_PATH" in os.environ:
        with open(os.environ["GITHUB_PATH"], "a", encoding="utf-8") as paths:
            paths.write(str(destination.resolve()) + "\n")
    print(f"Installed Tinymist {version}: {path}")


if __name__ == "__main__":
    install(sys.argv[1], Path(sys.argv[2]))
