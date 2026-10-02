"""Persistent PaddleOCR 3.x worker. stdin: u32 big-endian length + PNG; stdout: JSON lines."""
import os
import sys
import json
import struct

# Native libraries also print to stdout. Keep the protocol on a separate descriptor.
protocol = os.fdopen(os.dup(sys.stdout.fileno()), "w", buffering=1)
os.dup2(sys.stderr.fileno(), sys.stdout.fileno())


def reply(**data):
    protocol.write(json.dumps(data, ensure_ascii=False) + "\n")


def read_exact(size):
    data = bytearray()
    while len(data) < size:
        chunk = sys.stdin.buffer.read(size - len(data))
        if not chunk:
            raise EOFError("incomplete image")
        data.extend(chunk)
    return data


def main():
    import cv2
    import numpy as np
    from paddleocr import PaddleOCR

    ocr = PaddleOCR(lang=sys.argv[1], device="cpu", use_doc_orientation_classify=False,
                    use_doc_unwarping=False, use_textline_orientation=False)
    while True:
        header = sys.stdin.buffer.read(4)
        if not header:
            return
        if len(header) < 4:
            header += read_exact(4 - len(header))
        size, = struct.unpack(">I", header)
        if size == 0 or size > 128 * 1024 * 1024:
            raise ValueError("invalid PNG length")
        image = cv2.imdecode(np.frombuffer(read_exact(size), dtype=np.uint8), cv2.IMREAD_COLOR)
        if image is None:
            raise ValueError("invalid PNG")
        texts = []
        for result in ocr.predict(image):
            texts.extend(result["rec_texts"])
        reply(text="\n".join(texts))


try:
    main()
except Exception as error:
    reply(error=f"{type(error).__name__}: {error}")
    sys.exit(1)
