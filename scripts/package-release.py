#!/usr/bin/env python3
"""Package prebuilt Linux binaries with documentation and dependency notices."""
import hashlib
import json
import platform
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def command(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def main():
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
    version = manifest["package"]["version"]
    host = next(line.split(": ", 1)[1] for line in command("rustc", "-vV").splitlines() if line.startswith("host: "))
    if "linux" not in host:
        raise SystemExit("Este empacotador suporta builds Linux locais.")
    binaries = [ROOT / "target/release" / name for name in ("pomni", "pomni-gui")]
    if not all(path.is_file() for path in binaries):
        raise SystemExit("Compile primeiro: cargo build --release --locked --features gui --bins")
    if command("git", "status", "--porcelain"):
        raise SystemExit("Faça commit das alterações antes de gerar o pacote da release.")
    metadata = json.loads(command("cargo", "metadata", "--offline", "--locked", "--features", "gui", "--filter-platform", host, "--format-version", "1"))
    dist = ROOT / "dist"
    dist.mkdir(exist_ok=True)
    name = f"pomni-{version}-linux-{platform.machine()}"
    archive = dist / f"{name}.tar.gz"
    fallbacks = {"accesskit": "accesskit", "gl_generator": "gl-rs", "khronos_api": "gl-rs", "profiling": "profiling"}
    with tempfile.TemporaryDirectory(prefix="pomni-package-") as temp:
        stage = Path(temp) / name
        stage.mkdir()
        for source in [*binaries, ROOT / "README.md", ROOT / "LICENSE", ROOT / "CHANGELOG.md"]:
            shutil.copy2(source, stage / source.name)
        shutil.copytree(ROOT / "udev", stage / "udev")
        for asset in ["pomni.desktop", "pomni.svg"]:
            shutil.copy2(ROOT / "assets" / asset, stage / asset)
        licenses = stage / "licenses"
        licenses.mkdir()
        notices = ["# Third-party software notices", "", "Pomni's MIT license applies to its own code. Dependencies retain the licenses listed below.", "Full notices and font licenses are included in licenses/.", ""]
        for package in sorted(metadata["packages"], key=lambda p: (p["name"], p["version"])):
            if package["name"] == "pomni":
                continue
            crate = Path(package["manifest_path"]).parent
            destination = licenses / f"{package['name']}-{package['version']}"
            files = [path for path in crate.rglob("*") if path.is_file() and path.suffix != ".rs" and any(token in path.name.lower() for token in ("license", "copying", "notice", "copyright", "ofl.txt", "ufl.txt"))]
            for source in files:
                target = destination / source.relative_to(crate)
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, target)
            group = "egui" if "github.com/emilk/egui" in (package["repository"] or "") else fallbacks.get(package["name"])
            if group:
                shutil.copytree(ROOT / "assets/licenses" / group, destination / "upstream", dirs_exist_ok=True)
            if not files and not group:
                raise SystemExit(f"Licença não localizada para {package['name']}")
            notices += [f"## {package['name']} {package['version']}", f"License: {package['license'] or 'see included license file'}", f"Authors: {', '.join(package['authors'])}", f"Source: {package['repository'] or package['homepage'] or 'https://crates.io/crates/' + package['name']}", ""]
        shutil.copy2(ROOT / "assets/licenses/sources.json", licenses / "upstream-sources.json")
        (stage / "THIRD_PARTY_NOTICES.md").write_text("\n".join(notices))
        symbols = command("objdump", "-T", str(binaries[1]))
        import re
        glibc = max(set(re.findall(r"GLIBC_(\d+(?:\.\d+)+)", symbols)), key=lambda value: tuple(map(int, value.split("."))))
        info = f"Pomni {version}\nCommit: {command('git', 'rev-parse', 'HEAD')}\nTarget: {host}\nRust: {command('rustc', '--version')}\nMinimum GLIBC symbol version: {glibc}\nRuntime: Linux x86_64, OpenGL, Wayland or X11, libgcc_s.so.1\n"
        (stage / "BUILD-INFO.txt").write_text(info)
        with tarfile.open(archive, "w:gz", compresslevel=9) as output:
            output.add(stage, arcname=name)
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    (dist / "SHA256SUMS").write_text(f"{digest}  {archive.name}\n")
    print(archive)
    print(info)
    print(f"SHA256: {digest}")


if __name__ == "__main__":
    main()
