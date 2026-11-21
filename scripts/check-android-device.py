#!/usr/bin/env python3
"""Aragon Phase 4 device qualification (ARG-G05..G08) over adb.

Builds the device fixtures in crates/goya-sdk/tests/android-device/ with the
locked toolchain, signs the release APKs with a throwaway test key, installs
them on the one attached device or emulator, and checks:

  geometry   every element's on-screen bounds in the accessibility tree equal
             the Rust reference (examples/aragon_device_expect) at several
             display sizes and densities, in both orientations, and after
             scrolling each scroll viewport to its limit;
  behaviour  bound fields, field conversion and D1 dispatch, observed through
             the failure toast (a D1 overflow is the oracle for state);
  lifecycle  session state across activity recreation (rotation) and its
             reset after process death, with field text restored;
  input/a11y the soft keyboard opens on a field; buttons, fields and scroll
             viewports expose the expected accessibility roles and actions.

Exit status: 0 qualified, 1 a check failed, 2 environment missing.
Requires JAVA_HOME (locked JDK) and ANDROID_HOME (locked SDK). The device
display size, density and rotation are restored afterwards.
"""

import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import xml.etree.ElementTree as ET

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FIXTURES = os.path.join(REPO, "crates", "goya-sdk", "tests", "android-device")
PACKAGE = "dev.goya.preview"
ACTIVITY = PACKAGE + "/.MainActivity"
INT_MAX = "9223372036854775807"
OVERFLOW = "Action failed: ARITHMETIC_OVERFLOW"
# (size or None for the physical size, density or None for physical density)
# Drags per direction; enough to reach either scroll limit at every DISPLAYS entry.
SWIPES = 10
DISPLAYS = [(None, None), (None, 420), ("900x1600", 380), ("1200x1920", 480)]

failures = []


def env(name):
    value = os.environ.get(name)
    if not value or not os.path.isdir(value):
        print(f"environment: set {name}", file=sys.stderr)
        sys.exit(2)
    return value


SDK = env("ANDROID_HOME")
JAVA_HOME = env("JAVA_HOME")
ADB = os.path.join(SDK, "platform-tools", "adb")
BUILD_TOOLS = os.path.join(SDK, "build-tools", "34.0.0")
CHILD_ENV = dict(os.environ, PATH=os.path.join(JAVA_HOME, "bin") + os.pathsep + os.environ["PATH"])


def require_tools():
    """Fail as an environment issue before touching a connected device."""
    required = (ADB, os.path.join(BUILD_TOOLS, "zipalign"), os.path.join(BUILD_TOOLS, "apksigner"))
    missing = [path for path in required if not os.path.isfile(path) or not os.access(path, os.X_OK)]
    if missing:
        print("environment: missing executable Android tools: " + ", ".join(missing), file=sys.stderr)
        sys.exit(2)


require_tools()


def run(args, timeout=600, check=True, **kwargs):
    result = subprocess.run(args, capture_output=True, text=True, timeout=timeout, env=CHILD_ENV, **kwargs)
    if check and result.returncode != 0:
        raise RuntimeError(f"{' '.join(args)} failed:\n{result.stdout}\n{result.stderr}")
    return result.stdout


def adb(*args, timeout=120, check=True):
    return run([ADB, *args], timeout=timeout, check=check)


def shell(command, check=True):
    return adb("shell", command, check=check)


def check(condition, message):
    status = "ok  " if condition else "FAIL"
    print(f"  {status} {message}")
    if not condition:
        failures.append(message)
    return condition


def wait_idle(seconds=1.5):
    time.sleep(seconds)


# --- build -----------------------------------------------------------------

def build_apk(scene, work, key):
    project = os.path.join(work, os.path.basename(scene) + ".project")
    run([os.path.join(REPO, "target", "debug", "aragon-android-project"), scene, project, "--scene"])
    run(["./gradlew", "--no-daemon", "-q", "--console=plain", "--offline",
         "--dependency-verification", "strict", "assembleRelease"], cwd=project, timeout=1800)
    unsigned = os.path.join(project, "app", "build", "outputs", "apk", "release", "app-release-unsigned.apk")
    digest = run(["sha256sum", unsigned]).split()[0]
    aligned = os.path.join(work, os.path.basename(scene) + ".aligned.apk")
    signed = os.path.join(work, os.path.basename(scene) + ".apk")
    run([os.path.join(BUILD_TOOLS, "zipalign"), "-f", "-p", "4", unsigned, aligned])
    run([os.path.join(BUILD_TOOLS, "apksigner"), "sign", "--ks", key, "--ks-pass", "pass:aragon-test",
         "--out", signed, aligned])
    return signed, digest


# --- accessibility tree ----------------------------------------------------

def bounds(node):
    l, t, r, b = map(int, re.findall(r"-?\d+", node.attrib["bounds"]))
    return (l, t, r, b)


def dump():
    for _ in range(5):
        out = shell("uiautomator dump /sdcard/aragon.xml", check=False)
        if "dumped" in out:
            xml = adb("exec-out", "cat", "/sdcard/aragon.xml")
            return ET.fromstring(xml)
        wait_idle(1)
    raise RuntimeError("uiautomator dump failed")


def scene_root(tree):
    content = next(n for n in tree.iter("node") if n.attrib.get("resource-id") == "android:id/content")
    scene_view = list(content)[0]
    return list(scene_view)[0]


def expectations(scene, viewport, density, screen, scroll_max=()):
    args = [os.path.join(REPO, "target", "debug", "examples", "aragon_device_expect"), scene,
            *map(str, viewport), str(density), *map(str, screen)]
    for element, axes in scroll_max:
        args += ["--scroll-max", element, axes]
    rows = []
    for line in run(args).splitlines():
        f = line.split()
        rows.append({"id": f[0], "parent": f[1], "kind": f[2], "content": f[3], "scroll": f[4] == "1",
                     "bounds": tuple(map(int, f[7:11]))})
    return rows


CONTENT_CLASS = {"text": "android.widget.TextView", "button": "android.widget.Button",
                 "input": "android.widget.EditText"}


def visible(row):
    l, t, r, b = row["bounds"]
    return r > l and b > t


def match(node, row, rows, nodes, problems):
    """Walk one element's subtree; record element id -> node."""
    nodes[row["id"]] = node
    if node.attrib.get("class") != "android.view.ViewGroup":
        problems.append(f"{row['id']}: class {node.attrib.get('class')}")
    if bounds(node) != row["bounds"]:
        problems.append(f"{row['id']}: bounds {bounds(node)} != {row['bounds']}")
    children = list(node)
    if row["content"] != "none":
        content = children.pop(0) if children else None
        if content is None or content.attrib.get("class") != CONTENT_CLASS[row["content"]]:
            problems.append(f"{row['id']}: missing {row['content']} content")
        elif bounds(content) != row["bounds"]:
            problems.append(f"{row['id']}: content bounds {bounds(content)} != {row['bounds']}")
        else:
            nodes[row["id"] + "#content"] = content
    if row["scroll"]:
        layer = children.pop(0) if children else None
        if layer is None or layer.attrib.get("class") != "android.widget.ScrollView":
            problems.append(f"{row['id']}: missing scroll layer")
            return
        if bounds(layer) != row["bounds"]:
            problems.append(f"{row['id']}: layer bounds {bounds(layer)} != {row['bounds']}")
        nodes[row["id"] + "#layer"] = layer
        children = list(layer)
    # Accessibility orders siblings by screen position, not view order, so
    # pair each expected child with an unused node of the same bounds.
    expected = [r for r in rows if r["parent"] == row["id"] and visible(r)]
    if len(children) != len(expected):
        problems.append(f"{row['id']}: {len(children)} visible children, expected {len(expected)}")
    unused = list(children)
    for child_row in expected:
        candidates = [n for n in unused if bounds(n) == child_row["bounds"]
                      and n.attrib.get("class") == "android.view.ViewGroup"]
        if not candidates:
            problems.append(f"{child_row['id']}: no node with bounds {child_row['bounds']}")
            continue
        unused.remove(candidates[0])
        match(candidates[0], child_row, rows, nodes, problems)


def check_geometry(scene, label, density, scroll_max=()):
    tree = dump()
    root = scene_root(tree)
    viewport_bounds = bounds(root)
    viewport = (viewport_bounds[0], viewport_bounds[1],
                viewport_bounds[2] - viewport_bounds[0], viewport_bounds[3] - viewport_bounds[1])
    screen = bounds(tree[0])
    insets = shell("dumpsys window")
    bars = [tuple(map(int, m)) for m in re.findall(
        r"InsetsSource id=\w+ type=(?:statusBars|navigationBars|displayCutout)[^\n]*?frame=\[(-?\d+),(-?\d+)\]\[(-?\d+),(-?\d+)\][^\n]*visible=true",
        insets)]
    overlaps = [bar for bar in bars if bar[0] < viewport_bounds[2] and bar[2] > viewport_bounds[0]
                and bar[1] < viewport_bounds[3] and bar[3] > viewport_bounds[1]]
    check(not overlaps, f"{label}: viewport {viewport_bounds} clear of system bars")
    rows = expectations(scene, viewport, density, (screen[2], screen[3]), scroll_max)
    nodes, problems = {}, []
    match(root, rows[0], rows, nodes, problems)
    check(not problems, f"{label}: {len(nodes)} nodes match the reference" +
          ("" if not problems else "\n      " + "\n      ".join(problems[:8])))
    return tree, nodes, rows


# --- device actions ----------------------------------------------------------

def app_pid():
    level = int(shell("getprop ro.build.version.sdk").strip())
    for line in shell("ps -A" if level >= 26 else "ps").splitlines():
        fields = line.split()
        if fields and fields[-1] == PACKAGE:
            return fields[1]
    return None


def kill_in_background():
    """Process death as the low-memory killer causes it."""
    shell("input keyevent KEYCODE_HOME")
    wait_idle(2)
    for _ in range(3):
        shell(f"am kill {PACKAGE}")
        wait_idle(1)
        if app_pid() is None:
            return "am kill"
    pid = app_pid()
    if pid and shell(f"su 0 kill -9 {pid}", check=False) is not None:
        wait_idle(1)
        if app_pid() is None:
            return "kill -9 as root (am kill left the process alive)"
    return None


def install(apk):
    # Each run signs with a new throwaway key, so replace rather than update.
    adb("uninstall", PACKAGE, check=False)
    adb("install", apk)


def launch(fresh=True):
    if fresh:
        shell(f"am force-stop {PACKAGE}")
    shell(f"am start -W -n {ACTIVITY}")
    wait_idle()


def centre(node):
    l, t, r, b = bounds(node)
    return (l + r) // 2, (t + b) // 2


def tap(node):
    shell("input tap %d %d" % centre(node))
    wait_idle(0.8)


def ime_shown():
    return "mInputShown=true" in shell("dumpsys input_method")


def hide_ime():
    if ime_shown():
        shell("input keyevent KEYCODE_BACK")
        wait_idle(0.8)


def set_field(node, text):
    tap(node)
    shell("input keyevent KEYCODE_MOVE_END")
    for _ in range(30):
        shell("input keyevent KEYCODE_DEL")
    if text.startswith("-"):
        # `input text -0` would be read as an option.
        shell("input keyevent KEYCODE_MINUS")
        text = text[1:]
    if text:
        shell("input text " + text.replace(" ", "%s"))
    wait_idle(0.5)


seen_toasts = set()
# Android 11+ records toast text; older versions record only the package.
# The generated app shows toasts only for D1 failures.
UNKNOWN_TEXT = "<toast text not recorded by this Android version>"


def toasts():
    out = shell("dumpsys notification")
    found = re.findall(r"TextToastRecord\{(\w+) \d+:" + re.escape(PACKAGE) + r"/\S+ .*?text=(.*?) duration=", out)
    found += [(record, UNKNOWN_TEXT) for record in
              re.findall(r"(?<!Text)ToastRecord\{(\w+) pkg=" + re.escape(PACKAGE) + r" ", out)]
    fresh = [text for record, text in found if record not in seen_toasts]
    seen_toasts.update(record for record, _ in found)
    return fresh


def activate_expect(button, toast, label):
    toasts()
    tap(button)
    deadline = time.time() + (3.0 if toast else 2.5)
    shown = []
    while time.time() < deadline and not shown:
        shown = toasts()
        if not shown:
            time.sleep(0.25)
    if toast:
        return check(shown in ([toast], [UNKNOWN_TEXT]), f"{label}: toast {toast!r} (saw {shown})")
    return check(shown == [], f"{label}: no failure (saw {shown})")


# --- protocol ----------------------------------------------------------------

def main():
    if "device" not in adb("get-state", check=False):
        print("environment: no device or emulator attached", file=sys.stderr)
        return 2
    sdk_level = shell("getprop ro.build.version.sdk").strip()
    fingerprint = shell("getprop ro.build.fingerprint").strip()
    physical = re.search(r"Physical size: (\d+)x(\d+)", shell("wm size"))
    physical_density = int(re.search(r"Physical density: (\d+)", shell("wm density")).group(1))
    print(f"device: API {sdk_level}, {fingerprint}, {physical.group(1)}x{physical.group(2)} @ {physical_density}")

    work = tempfile.mkdtemp(prefix="aragon-device-")
    original_rotation = shell("settings get system user_rotation").strip()
    original_auto = shell("settings get system accelerometer_rotation").strip()
    try:
        run(["cargo", "build", "--offline", "-q", "-p", "goya-sdk", "--bins", "--examples"], cwd=REPO, timeout=1800)
        key = os.path.join(work, "test.keystore")
        run([os.path.join(JAVA_HOME, "bin", "keytool"), "-genkeypair", "-keystore", key, "-storepass", "aragon-test",
             "-keypass", "aragon-test", "-alias", "test", "-keyalg", "RSA", "-keysize", "2048", "-validity", "1",
             "-dname", "CN=Aragon qualification test key"])
        scenes = {name: os.path.join(FIXTURES, name + ".gscene") for name in ("layout", "counter")}
        apks = {}
        for name, scene in scenes.items():
            apks[name], digest = build_apk(scene, work, key)
            print(f"built {name}: unsigned release APK sha256 {digest}")
        shell("settings put system accelerometer_rotation 0")
        shell("settings put system user_rotation 0")

        # Geometry: layout fixture across display configurations.
        install(apks["layout"])
        layout = scenes["layout"]
        for size, density in DISPLAYS:
            shell("wm size " + (size or "reset"))
            shell("wm density " + (str(density) if density else "reset"))
            wait_idle(2)
            effective = density or physical_density
            label = f"layout {size or 'physical'} @ {effective}"
            launch()
            _, nodes, _ = check_geometry(layout, label, effective / 160)
            # Scroll each viewport to its limit by dragging, then back.
            for element, axis in (("vscroll", "y"), ("hscroll", "x")):
                layer = nodes[element + "#layer"]
                l, t, r, b = bounds(layer)
                cx, cy = (l + r) // 2, (t + b) // 2
                for _ in range(SWIPES):
                    if axis == "y":
                        shell(f"input swipe {cx} {b - 10} {cx} {t + 10} 300")
                    else:
                        shell(f"input swipe {r - 10} {cy} {l + 10} {cy} 300")
                wait_idle(1)
                check_geometry(layout, f"{label}, {element} at its scroll limit", effective / 160,
                               ((element, axis),))
                for _ in range(SWIPES):
                    if axis == "y":
                        shell(f"input swipe {cx} {t + 10} {cx} {b - 10} 300")
                    else:
                        shell(f"input swipe {l + 10} {cy} {r - 10} {cy} 300")
                wait_idle(1)
                tree, nodes, _ = check_geometry(layout, f"{label}, {element} back at 0", effective / 160)
        shell("wm size reset")
        shell("wm density reset")
        wait_idle(2)
        density = physical_density / 160

        # Landscape geometry (activity recreated with a new viewport).
        shell("settings put system user_rotation 1")
        wait_idle(2.5)
        check_geometry(layout, f"layout landscape @ {physical_density}", density)
        shell("settings put system user_rotation 0")
        wait_idle(2.5)

        # Accessibility roles and actions.
        launch()
        tree, nodes, _ = check_geometry(layout, "layout a11y pass", density)
        button = nodes["grid.button#content"]
        check(button.attrib.get("clickable") == "true" and button.attrib.get("text") == "Tap",
              "button: clickable with its label as text")
        layer = nodes["vscroll#layer"]
        check(layer.attrib.get("scrollable") == "true", "scroll viewport: scrollable for accessibility")

        # Behaviour and lifecycle: counter fixture.
        install(apks["counter"])
        counter = scenes["counter"]
        launch()
        _, nodes, _ = check_geometry(counter, "counter portrait", density)
        amount, add = nodes["field.amount#content"], nodes["button.add#content"]
        check(amount.attrib.get("focusable") == "true" and amount.attrib.get("NAF") == "true",
              "field: focusable; has no accessible name (no label in GSSH/2; recorded gap)")

        tap(amount)
        check(ime_shown(), "soft keyboard shown when a field is tapped")
        set_field(amount, INT_MAX)
        hide_ime()
        activate_expect(add, None, "Add max: total = MAX")
        activate_expect(add, OVERFLOW, "Add max again: checked overflow, state kept")
        for rejected in ("01", "-0", "+1", " 1", "1.0", "abc"):
            set_field(amount, rejected)
            hide_ime()
            activate_expect(add, None, f"amount {rejected!r} refused before dispatch")
        set_field(amount, "1")
        hide_ime()
        activate_expect(add, OVERFLOW, "refused fields changed nothing: total still MAX")
        set_field(amount, "-1")
        hide_ime()
        activate_expect(add, None, "Add -1: total = MAX-1")
        set_field(amount, "1")
        hide_ime()
        activate_expect(add, None, "Add 1: total = MAX")
        activate_expect(add, OVERFLOW, "Add 1 again overflows: arithmetic matches D1")

        # Recreation: rotate; session state and field text survive.
        shell("settings put system user_rotation 1")
        wait_idle(2.5)
        tree, nodes, _ = check_geometry(counter, "counter landscape after recreation", density)
        check(nodes["field.amount#content"].attrib.get("text") == "1", "field text restored after recreation")
        activate_expect(nodes["button.add#content"], OVERFLOW, "session state survived recreation")
        shell("settings put system user_rotation 0")
        wait_idle(2.5)

        # Process death: state resets to its initial value; field text restored.
        method = kill_in_background()
        check(method is not None, f"process killed in the background ({method})")
        launch(fresh=False)
        tree, nodes, _ = check_geometry(counter, "counter after process death", density)
        amount, add = nodes["field.amount#content"], nodes["button.add#content"]
        check(amount.attrib.get("text") == "1", "field text restored after process death")
        activate_expect(add, None, "Add 1 after process death: total reset to 0, now 1")
        set_field(amount, INT_MAX)
        hide_ime()
        activate_expect(add, OVERFLOW, "Add max: 1 + MAX overflows, so the reset was to 0")
    finally:
        shell("wm size reset", check=False)
        shell("wm density reset", check=False)
        for key, value in (("user_rotation", original_rotation), ("accelerometer_rotation", original_auto)):
            if value.isdigit():
                shell(f"settings put system {key} {value}", check=False)
        shutil.rmtree(work, ignore_errors=True)

    if failures:
        print(f"\n{len(failures)} check(s) failed.")
        return 1
    print("\nDevice qualification checks passed on this device.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
