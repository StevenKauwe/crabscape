"""Tiny PNG I/O through ffmpeg (no PIL needed). Arrays are float32 0..1, HxWxC."""
import subprocess, json
import numpy as np

def size(path):
    out = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries",
                          "stream=width,height", "-of", "json", path], capture_output=True, check=True).stdout
    s = json.loads(out)["streams"][0]
    return s["width"], s["height"]

def load(path, alpha=False):
    w, h = size(path)
    fmt = "rgba" if alpha else "rgb24"
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", path, "-f", "rawvideo", "-pix_fmt", fmt, "-"],
                         capture_output=True, check=True).stdout
    return np.frombuffer(raw, np.uint8).reshape(h, w, 4 if alpha else 3).astype(np.float32) / 255.0

def save(path, img):
    h, w, c = img.shape
    fmt = {3: "rgb24", 4: "rgba"}[c]
    data = (np.clip(img, 0, 1) * 255 + 0.5).astype(np.uint8).tobytes()
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", fmt, "-s", f"{w}x{h}",
                    "-i", "-", "-frames:v", "1", path], input=data, check=True)
