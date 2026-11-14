"""Copy the locked Rust dependency notices into the distributable app bundle."""
import json
import shutil
import subprocess
import sys
from pathlib import Path

out = Path(sys.argv[1])
metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--format-version", "1"], text=True))
for package in metadata["packages"]:
    if package["source"] is None:
        continue
    source = Path(package["manifest_path"]).parent
    destination = out / f'{package["name"]}-{package["version"]}'
    for item in source.iterdir():
        if item.is_file() and item.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE", "COPYRIGHT")):
            destination.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(item, destination / item.name)
sysroot = Path(subprocess.check_output(["rustc", "--print", "sysroot"], text=True).strip())
notice = sysroot / "share/doc/rust/COPYRIGHT"
if notice.is_file():
    out.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(notice, out / "Rust-COPYRIGHT")
