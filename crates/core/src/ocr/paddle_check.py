import contextlib
import importlib.metadata
import json
import os
from pathlib import Path
import sys

# Keep native library chatter out of JSON stdout.
protocol = os.fdopen(os.dup(sys.stdout.fileno()), "w")
os.dup2(sys.stderr.fileno(), sys.stdout.fileno())
try:
    import paddle
    import paddleocr
    version = importlib.metadata.version("paddleocr")
    ready = version.split(".")[0] == "3"
    detail = f"PaddleOCR {version}; PaddlePaddle {paddle.__version__}" + ("" if ready else "; требуется PaddleOCR 3.x")
except Exception as error:
    ready = False
    detail = f"{type(error).__name__}: {error}"
models = []
for root in [Path.home() / ".paddlex/official_models", Path.home() / ".paddleocr/whl"]:
    if root.exists():
        for params in root.rglob("*.pdiparams"):
            models.append(params.parent.name)
protocol.write(json.dumps(dict(ready=ready, detail=detail, models=sorted(set(models))), ensure_ascii=False))
protocol.close()
