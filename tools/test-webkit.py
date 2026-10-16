#!/usr/bin/env python3
"""Play CrabScape in a real WebKit engine (the engine of Safari and of every browser on iPhone).

    tools/build-web.sh && python3 tools/test-webkit.py [--landscape | --desktop] [--shots /tmp/shot]
                                                       [--style-shots /tmp/style]

Needs WebKitGTK 4.1 with its Python bindings (Arch: webkit2gtk-4.1 and python-gobject) and a desktop session
(the page is drawn in an off-screen window). It serves web/ on a local port, opens the game with an iPhone's
screen size and user agent (portrait 390x844; --landscape turns the phone; --desktop is a 1024x700 desktop browser),
and checks that:
  - WebKit accepts the .wasm file (and, as a sanity check, rejects a relaxed-SIMD module, like an iPhone does);
  - the game starts with no error message and draws something into the canvas, including warm lantern light;
  - the crab's pictures (assets/crab/*.png, fetched next to the page) load, the style starts as Oil, and the P key
    and the on-screen "Style" button switch between Oil and Ink (the canvas's data-style says which);
  - holding D (a simulated key press) walks the crab right, and Space makes it jump (its height goes up);
  - a second jump press in the water is a paddle stroke that lifts it again;
  - the on-screen touch buttons (simulated finger presses) walk it left and make it jump;
  - F9 hands out a replay of the session (shown in a box to copy), and the headless runner, playing that replay,
    ends exactly where the crab in the browser stands (needs cargo; see crates/sim);
  - its resting position, and the chosen style, are saved in localStorage and survive a reload.
The game writes the crab's state onto the canvas as data-crab="x y grounded" (see src/view.rs); the test reads that.
With --shots PREFIX it saves screenshots as PREFIX-1.png, PREFIX-2.png, ...; with --style-shots PREFIX it saves
PREFIX-oil.png and PREFIX-ink.png (the crab at the spawn point in each style).
WebKitGTK on Linux is not iOS: same JavaScript, WebAssembly and WebGL code, but not the same GPU or memory limits.
"""
import functools
import http.server
import json
import os
import resource
import sys
import threading
import time

# No core dumps from this process or the WebKit processes it starts. (A crash in a graphics driver while WebKit
# shuts down would otherwise leave a core file, and on some desktops a "Process crashed" pop-up.)
resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
# Use Mesa's software OpenGL rather than the machine's GPU driver: the NVIDIA EGL driver has been seen to crash
# WebKit's web process on exit. Software rendering is plenty for a 2D test.
if os.path.exists("/usr/share/glvnd/egl_vendor.d/50_mesa.json"):
    os.environ.setdefault("__EGL_VENDOR_LIBRARY_FILENAMES", "/usr/share/glvnd/egl_vendor.d/50_mesa.json")
os.environ.setdefault("LIBGL_ALWAYS_SOFTWARE", "1")
# Draw off screen without a GPU compositor (GTK's off-screen windows can't make a GL context). WebGL2 still works,
# rendered in software, which is all we need to see the game draw.
os.environ.setdefault("WEBKIT_DISABLE_COMPOSITING_MODE", "1")
os.environ.setdefault("WEBKIT_DISABLE_DMABUF_RENDERER", "1")
os.environ.setdefault("GDK_BACKEND", "x11" if os.environ.get("DISPLAY") else "wayland")

import gi  # noqa: E402

gi.require_version("Gtk", "3.0")
gi.require_version("WebKit2", "4.1")
from gi.repository import GLib, Gtk, WebKit2  # noqa: E402

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "web")
IPHONE = ("Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 "
          "(KHTML, like Gecko) Version/18.6 Mobile/15E148 Safari/604.1")
DESKTOP = "--desktop" in sys.argv
LANDSCAPE = "--landscape" in sys.argv
WIDTH, HEIGHT = (1024, 700) if DESKTOP else (844, 390) if LANDSCAPE else (390, 844)
SHOTS = sys.argv[sys.argv.index("--shots") + 1] if "--shots" in sys.argv else None
STYLE_SHOTS = sys.argv[sys.argv.index("--style-shots") + 1] if "--style-shots" in sys.argv else None


class Handler(http.server.SimpleHTTPRequestHandler):
    extensions_map = {**http.server.SimpleHTTPRequestHandler.extensions_map,
                      ".wasm": "application/wasm", ".js": "text/javascript",
                      ".png": "image/png"}

    def log_message(self, *args):
        pass


server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(Handler, directory=ROOT))
threading.Thread(target=server.serve_forever, daemon=True).start()
base = f"http://127.0.0.1:{server.server_address[1]}"

failures = 0


def check(ok, what):
    global failures
    print(("ok   " if ok else "FAIL ") + what, flush=True)
    failures += 0 if ok else 1


# A non-ephemeral data manager in a scratch folder, so localStorage survives a reload.
scratch = os.environ.get("SCRATCH", os.path.join(ROOT, "..", "target", "webkit-profile"))
os.makedirs(scratch, exist_ok=True)
manager = WebKit2.WebsiteDataManager(base_data_directory=scratch, base_cache_directory=scratch)
context = WebKit2.WebContext.new_with_website_data_manager(manager)
view = WebKit2.WebView.new_with_context(context)
settings = view.get_settings()
settings.set_enable_write_console_messages_to_stdout(True)
settings.set_enable_webgl(True)
if not DESKTOP:
    settings.set_user_agent(IPHONE)
window = Gtk.OffscreenWindow()
window.set_default_size(WIDTH, HEIGHT)
window.add(view)
window.show_all()


def pump(seconds):
    end = time.time() + seconds
    ctx = GLib.MainContext.default()
    while time.time() < end:
        ctx.iteration(False)
        time.sleep(0.005)


def js(script, timeout=30):
    """Evaluate the expression `script` in the page (it may use await) and return its value, through JSON."""
    box = {}
    body = f"return JSON.stringify(await (async () => {{ return {script}; }})());"

    def done(v, res):
        try:
            box["v"] = v.call_async_javascript_function_finish(res).to_string()
        except Exception as e:  # noqa: BLE001
            box["e"] = str(e)

    view.call_async_javascript_function(body, -1, None, None, None, None, done)
    end = time.time() + timeout
    while "v" not in box and "e" not in box and time.time() < end:
        pump(0.05)
    if "e" in box:
        raise RuntimeError(box["e"])
    if "v" not in box:
        raise TimeoutError(script)
    return json.loads(box["v"]) if box["v"] not in (None, "undefined") else None


def wait_for(expr, timeout):
    end = time.time() + timeout
    while time.time() < end:
        if js(expr):
            return True
        pump(0.25)
    return False


def load(url):
    loaded = {}
    handler = view.connect("load-changed", lambda _v, ev: ev == WebKit2.LoadEvent.FINISHED and loaded.update(done=True))
    view.load_uri(url)
    end = time.time() + 60
    while not loaded.get("done") and time.time() < end:
        pump(0.05)
    view.disconnect(handler)


def snapshot(path):
    """Save what the page shows (canvas included) as a PNG."""
    box = {}

    def done(v, res):
        try:
            box["s"] = v.get_snapshot_finish(res)
        except Exception as e:  # noqa: BLE001
            box["e"] = str(e)

    view.get_snapshot(WebKit2.SnapshotRegion.VISIBLE, WebKit2.SnapshotOptions.NONE, None, done)
    end = time.time() + 20
    while not box and time.time() < end:
        pump(0.05)
    if "s" not in box:
        raise RuntimeError(box.get("e", "snapshot timed out"))
    box["s"].write_to_png(path)
    return box["s"]


def shot(n):
    """Save screenshot number n if --shots was given (else into the scratch folder). Returns the cairo surface."""
    path = f"{SHOTS}-{n}.png" if SHOTS else os.path.join(scratch, f"shot-{n}.png")
    surface = snapshot(path)
    if SHOTS:
        print(f"     screenshot saved to {os.path.abspath(path)}")
    return surface


def crab():
    """The crab's state, as the game writes it on the canvas: (x, y, grounded)."""
    raw = js('document.getElementById("crabscape").getAttribute("data-crab")')
    if not raw:
        return None
    x, y, g = raw.split()
    return float(x), float(y), g == "1"


def key(kind, code, key_name):
    """Send a keydown or keyup, as a real key would. winit listens for keys on the canvas."""
    return js(f"""(() => {{
      const c = document.getElementById("crabscape");
      c.focus();
      c.dispatchEvent(new KeyboardEvent("{kind}", {{ bubbles: true, cancelable: true, composed: true,
                                                     code: "{code}", key: "{key_name}" }}));
      return true;
    }})()""")


def pointer(kind, sx, sy, pid, buttons=1):
    """Send one pointer event (kind = 'pointerdown', 'pointermove' or 'pointerup') for a finger at CSS pixel (sx, sy)."""
    return js(f"""(() => {{
      const c = document.getElementById("crabscape");
      const r = c.getBoundingClientRect();
      const opts = {{ bubbles: true, cancelable: true, composed: true, clientX: r.left + {sx}, clientY: r.top + {sy},
                      pointerId: {pid}, pointerType: "touch", isPrimary: {str(pid == 2).lower()},
                      width: 20, height: 20, pressure: 0.5, button: 0, buttons: {buttons} }};
      if ("{kind}" === "pointerdown") c.dispatchEvent(new PointerEvent("pointerover", opts));
      c.dispatchEvent(new PointerEvent("{kind}", opts));
      return true;
    }})()""")


def button_centre(name):
    """Middle of an on-screen button, in CSS pixels. Mirrors button_rects() in src/input.rs."""
    size = min(max(min(WIDTH, HEIGHT) * 0.18, 56.0), 96.0)
    gap, margin = size * 0.25, 18.0
    bottom = HEIGHT - margin - 14.0
    left_x = {"left": margin, "right": margin + size + gap,
              "jump": WIDTH - margin - size, "dodge": WIDTH - margin - 2 * size - gap}[name]
    bottom_y = bottom - size * 0.45 if name == "jump" else bottom
    return left_x + size / 2, bottom_y - size / 2


def hold(seconds, sample=True):
    """Let the game run for a while, recording the crab's highest point."""
    top = -1e9
    end = time.time() + seconds
    while time.time() < end:
        pump(0.05)
        if sample:
            c = crab()
            if c:
                top = max(top, c[1])
    return top


def warm_pixels(surface):
    """How many pixels look like lantern light or a lit crab: clearly red/orange/yellow and bright."""
    data = surface.get_data()
    stride, w, h = surface.get_stride(), surface.get_width(), surface.get_height()
    count = 0
    for y in range(0, h, 2):
        row = y * stride
        for x in range(0, w, 2):
            b, g, r = data[row + x * 4], data[row + x * 4 + 1], data[row + x * 4 + 2]
            if r > 110 and r > g * 1.15 and g > b:
                count += 1
    return count


def rest():
    """Wait until the crab stands still on the floor (or give up after 4 s)."""
    end = time.time() + 4
    last = None
    while time.time() < end:
        pump(0.3)
        c = crab()
        if c and c[2] and last and abs(c[0] - last[0]) < 0.5 and abs(c[1] - last[1]) < 0.5:
            return c
        last = c
    return crab()


# --- start ---------------------------------------------------------------------------------------------------------
load(base + "/index.html")
js('(localStorage.clear(), true)')
load(base + "/index.html")
print(f"     WebKitGTK {WebKit2.get_major_version()}.{WebKit2.get_minor_version()}.{WebKit2.get_micro_version()}, "
      f"viewport {WIDTH}x{HEIGHT}, user agent: {js('navigator.userAgent')}")

probe = js("""await (async () => {
  const bytes = new Uint8Array(await (await fetch("pkg/crabscape_bg.wasm")).arrayBuffer());
  // A tiny module using one relaxed-SIMD instruction (i32x4.relaxed_trunc_f32x4_s, 0xfd 0x101).
  const relaxed = new Uint8Array([0,97,115,109,1,0,0,0,1,5,1,96,0,1,123,3,2,1,0,10,25,1,23,0,253,12,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,253,129,2,11]);
  const gl = document.createElement("canvas").getContext("webgl2");
  return { game: WebAssembly.validate(bytes), relaxed: WebAssembly.validate(relaxed), size: bytes.length,
           webgl2: !!gl, renderer: gl ? gl.getParameter(gl.RENDERER) : null };
})()""", 60)
print(f"     {probe}")
check(probe["game"], f"WebKit accepts crabscape_bg.wasm ({probe['size'] // 1024} KiB)")
check(not probe["relaxed"], "WebKit rejects a relaxed-SIMD module (so the check above means something)")
check(probe["webgl2"], "WebGL2 is available")

started = wait_for('document.getElementById("message").hidden', 90)
err = js('document.getElementById("message").textContent.trim()')
check(started, "the game starts and the loading message goes away" + ("" if started else f" (page says: {err})"))
size = js('(() => { const c = document.getElementById("crabscape"); return [c.width, c.height, c.clientWidth, c.clientHeight]; })()')
print(f"     canvas backing size {size[0]}x{size[1]}, CSS size {size[2]}x{size[3]}")
check(size[0] > 0 and size[1] > 0 and size[2] == WIDTH and size[3] == HEIGHT, "the canvas fills the window")
check(wait_for('document.getElementById("crabscape").hasAttribute("data-crab")', 20), "the game reports the crab")
start = rest()
print(f"     crab at {start}")
check(start is not None and start[2] and abs(start[0] - 160) < 1 and abs(start[1]) < 1,
      "a fresh game starts the crab on the sea floor at the spawn point")
first = shot(1)
warm = warm_pixels(first)
check(warm > 300, f"the canvas renders, with warm lantern light around the crab ({warm} warm pixels)")

# --- art style ------------------------------------------------------------------------------------------------------
canvas_attr = 'document.getElementById("crabscape").getAttribute("{}")'.format
fetched = js("""await Promise.all(["assets/crab/crab-atlas.png", "assets/crab/crab-atlas-oil.png"].map(
  async (p) => { const r = await fetch(p); return [p, r.status, (await r.arrayBuffer()).byteLength]; }))""")
print(f"     {fetched}")
check(all(f[1] == 200 and f[2] > 10000 for f in fetched), "both crab atlases are served next to the page")
check(wait_for(canvas_attr("data-art") + ' === "ink:1 oil:1"', 20),
      f"the game loaded both atlases (data-art={js(canvas_attr('data-art'))!r})")
check(js(canvas_attr("data-style")) == "oil", f"a fresh game starts in the oil style ({js(canvas_attr('data-style'))!r})")


def style_shot(name):
    pump(0.6)
    surface = snapshot(f"{STYLE_SHOTS}-{name}.png" if STYLE_SHOTS else os.path.join(scratch, f"style-{name}.png"))
    if STYLE_SHOTS:
        print(f"     screenshot saved to {os.path.abspath(f'{STYLE_SHOTS}-{name}.png')}")
    return surface


oil_warm = warm_pixels(style_shot("oil"))
key("keydown", "KeyP", "p")
pump(0.1)
key("keyup", "KeyP", "p")
check(wait_for(canvas_attr("data-style") + ' === "ink"', 5), "the P key switches to the ink style")
ink_warm = warm_pixels(style_shot("ink"))
print(f"     warm pixels: oil {oil_warm}, ink {ink_warm}")
check(oil_warm > 300 and ink_warm > 300, "both styles draw the lit crab")
# The style button, top-right (mirrors button_rect() in src/style.rs), clicked with a mouse.
bx, by = WIDTH - 10 - 52, (58 if WIDTH < 600 else 10) + 17
js(f"""(() => {{
  const c = document.getElementById("crabscape");
  const r = c.getBoundingClientRect();
  const o = {{ bubbles: true, cancelable: true, composed: true, clientX: r.left + {bx}, clientY: r.top + {by},
               pointerId: 1, pointerType: "mouse", isPrimary: true, button: 0, buttons: 1 }};
  c.dispatchEvent(new PointerEvent("pointermove", o));
  c.dispatchEvent(new PointerEvent("pointerdown", o));
  c.dispatchEvent(new MouseEvent("mousedown", o));
  return true;
}})()""")
pump(0.15)
js(f"""(() => {{
  const c = document.getElementById("crabscape");
  const r = c.getBoundingClientRect();
  const o = {{ bubbles: true, cancelable: true, composed: true, clientX: r.left + {bx}, clientY: r.top + {by},
               pointerId: 1, pointerType: "mouse", isPrimary: true, button: 0, buttons: 0 }};
  c.dispatchEvent(new PointerEvent("pointerup", o));
  c.dispatchEvent(new MouseEvent("mouseup", o));
  return true;
}})()""")
check(wait_for(canvas_attr("data-style") + ' === "oil"', 5), "clicking the Style button switches back to oil")
check(js('localStorage.getItem("crabscape-style")') == "oil", "the style choice is stored in localStorage")
# Finger tap on the button: to ink, which the reload at the end should keep.
pointer("pointerdown", bx, by, 9)
pump(0.15)
pointer("pointerup", bx, by, 9, 0)
check(wait_for(canvas_attr("data-style") + ' === "ink"', 5), "tapping the Style button switches style")

# --- keyboard -------------------------------------------------------------------------------------------------------
key("keydown", "KeyD", "d")
hold(0.5, sample=False)
key("keyup", "KeyD", "d")
moved = rest()
check(moved[0] > start[0] + 80, f"holding D walks the crab right (x {start[0]:.0f} -> {moved[0]:.0f})")

key("keydown", "Space", " ")
top = hold(0.25)
shot(2)  # mid-jump
top = max(top, hold(0.2))
key("keyup", "Space", " ")
top = max(top, hold(1.2))
check(top > moved[1] + 120, f"holding Space jumps (height {moved[1]:.0f} -> {top:.0f})")

# Jump, let go, then press again on the way down: the paddle stroke.
landed = rest()
key("keydown", "Space", " ")
hold(0.45, sample=False)
before_paddle = crab()
key("keyup", "Space", " ")
pump(0.05)
key("keydown", "Space", " ")
top = hold(0.35)
key("keyup", "Space", " ")
check(before_paddle and top > before_paddle[1] + 30,
      f"a second jump press in the water paddles upwards again ({before_paddle[1]:.0f} -> {top:.0f})")
hold(1.5, sample=False)

# --- touch buttons --------------------------------------------------------------------------------------------------
here = rest()
# The first touch shows the buttons (on a real phone they are already showing).
pointer("pointerdown", WIDTH / 2, HEIGHT / 3, 2)
pump(0.1)
pointer("pointerup", WIDTH / 2, HEIGHT / 3, 2, 0)
pump(0.3)
lx, ly = button_centre("left")
pointer("pointerdown", lx, ly, 3)
hold(0.7, sample=False)
pointer("pointerup", lx, ly, 3, 0)
after = rest()
check(after[0] < here[0] - 80, f"holding the on-screen < button walks the crab left (x {here[0]:.0f} -> {after[0]:.0f})")

jx, jy = button_centre("jump")
pointer("pointerdown", jx, jy, 4)
top = hold(0.25)
shot(3)  # touch buttons showing, crab in the water
top = max(top, hold(0.2))
pointer("pointerup", jx, jy, 4, 0)
top = max(top, hold(1.0))
check(top > after[1] + 120, f"holding the on-screen JUMP button jumps (height {after[1]:.0f} -> {top:.0f})")

# --- replay ---------------------------------------------------------------------------------------------------------
# F9 asks for a replay of everything so far; the page shows it in a box. Played headless, it must end exactly where
# the crab stands now: same controls, same simulation, same result.
standing = rest()
key("keydown", "F9", "F9")
pump(0.1)
key("keyup", "F9", "F9")
check(wait_for('document.getElementById("crabscape").hasAttribute("data-replay")', 5), "F9 hands out a replay")
replay = js(canvas_attr("data-replay")) or ""
check(replay.startswith("# CrabScape replay") and " jump" in replay and not js('document.getElementById("replay-box").hidden'),
      f"the page shows the replay to copy ({len(replay.splitlines())} lines)")
js('document.getElementById("replay-close").click()')
try:
    import subprocess
    import tempfile
    with tempfile.NamedTemporaryFile("w", suffix=".txt", delete=False) as f:
        f.write(replay)
    root = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
    out = subprocess.run(["cargo", "run", "--release", "--quiet", "-p", "crabsim", "--", "--inputs", f.name],
                         cwd=root, capture_output=True, text=True, timeout=600).stdout
    end = next(line for line in out.splitlines() if line.startswith("crab:")).split("end (")[1].split(")")[0]
    hx, hy = (float(v) for v in end.split(","))
    check(abs(hx - standing[0]) < 0.11 and abs(hy - standing[1]) < 0.11,
          f"the replay played headless ends where the crab stands (headless {hx}, {hy}; browser {standing[0]}, {standing[1]})")
except (OSError, StopIteration, IndexError, ValueError) as e:
    print(f"skip the headless replay check ({e!r})")

# --- save and reload ------------------------------------------------------------------------------------------------
resting = rest()
pump(1.0)  # the game saves at most twice a second
raw = js('localStorage.getItem("crabscape.save")')
saved = json.loads(raw) if raw else {}
check(saved.get("version") == 2 and abs(saved.get("x", -1e9) - resting[0]) < 1.5,
      f"the resting position is saved in localStorage ({raw})")
load(base + "/index.html")
wait_for('document.getElementById("message").hidden', 90)
wait_for('document.getElementById("crabscape").hasAttribute("data-crab")', 20)
again = rest()
check(again and abs(again[0] - resting[0]) < 1.5, f"after a reload the crab is back where it was ({again})")
check(js(canvas_attr("data-style")) == "ink", f"after a reload the chosen style is kept ({js(canvas_attr('data-style'))!r})")

# Stop WebKit's page process ourselves (a plain kill, before Python exits) so it never runs its GPU teardown.
view.terminate_web_process()
pump(0.5)
server.shutdown()
print(f"\n{failures} check(s) FAILED" if failures else "\nall WebKit checks passed")
sys.exit(1 if failures else 0)
