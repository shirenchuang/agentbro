import base64
import hashlib
import json
import sys
from importlib import metadata
from pathlib import Path

try:
    if any((Path(sys.prefix) / receipt).exists()
           for receipt in ("uv-receipt.toml", "pipx_metadata.json")):
        sys.exit(0)
    metadata.distribution("pip")
    package = metadata.distribution("aider-chat")
    launcher = Path(sys.argv[1])
    for entry in package.files or ():
        if entry.name.lower() not in ("aider", "aider.exe") or not entry.hash:
            continue
        path = Path(package.locate_file(entry))
        try:
            if not path.samefile(launcher):
                continue
        except OSError:
            continue
        fingerprint = hashlib.new(entry.hash.mode, path.read_bytes()).digest()
        if base64.urlsafe_b64encode(fingerprint).decode().rstrip("=") != entry.hash.value:
            continue
        print("agentbro-aider " + json.dumps({
            "python": sys.executable,
            "site": str(package.locate_file("")),
            "version": package.version,
        }))
        break
except (metadata.PackageNotFoundError, OSError, ValueError):
    pass
