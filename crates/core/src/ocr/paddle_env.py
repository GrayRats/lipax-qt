"""Facts about the PaddleOCR environment of one Python, as JSON. It decides nothing: the application does.

Looks at the package metadata first (cheap and always possible), and imports the packages only if they are
installed: an import is the real test (a wheel built for another Python or a missing library fails there)
but it takes seconds and prints, so the protocol goes to a separate descriptor.
"""
import importlib.metadata as metadata
import json
import os
import sys
from pathlib import Path

protocol = os.fdopen(os.dup(sys.stdout.fileno()), "w")
os.dup2(sys.stderr.fileno(), sys.stdout.fileno())


def read(path, limit=2000):
    try:
        return Path(path).read_text(encoding="utf-8", errors="replace")[:limit]
    except OSError:
        return ""


def distribution(*names):
    for name in names:
        try:
            found = metadata.distribution(name)
        except metadata.PackageNotFoundError:
            continue
        direct = None
        try:
            direct = json.loads(found.read_text("direct_url.json") or "null")
        except (ValueError, OSError):
            pass
        return {
            "name": name,
            "version": found.version,
            "location": str(found.locate_file("")),
            "direct_url": direct,
            "installer": (found.read_text("INSTALLER") or "").strip(),
        }
    return None


facts = {
    "python_version": sys.version.split()[0],
    "executable": sys.executable,
    "prefix": sys.prefix,
    "venv": sys.prefix != sys.base_prefix,
    "pyvenv_cfg": read(Path(sys.prefix) / "pyvenv.cfg"),
    "paddleocr": distribution("paddleocr"),
    "paddle": distribution("paddlepaddle", "paddlepaddle-gpu", "paddlepaddle-cpu"),
    "import_errors": {},
    "models": [],
}
for module, key in (("paddle", "paddle"), ("paddleocr", "paddleocr")):
    if facts[key] is None:
        continue
    try:
        __import__(module)
    except Exception as error:  # a broken install can fail in any way
        facts["import_errors"][module] = f"{type(error).__name__}: {error}"
for root in (Path.home() / ".paddlex/official_models", Path.home() / ".paddleocr/whl"):
    if root.exists():
        facts["models"].extend(p.parent.name for p in root.rglob("*.pdiparams"))
facts["models"] = sorted(set(facts["models"]))
protocol.write(json.dumps(facts, ensure_ascii=False))
protocol.close()
