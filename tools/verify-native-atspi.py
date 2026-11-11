#!/usr/bin/env python3
"""Explicit AT-SPI carrier acceptance using only an owned synthetic helper."""
import argparse
import json
import queue
import re
import subprocess
import threading
import time


def run(helper):
    def session_call(*args):
        return subprocess.check_output(["gdbus", "call", "--session", *args], text=True, timeout=3)

    address_result = session_call("--dest", "org.a11y.Bus", "--object-path", "/org/a11y/bus", "--method", "org.a11y.Bus.GetAddress")
    address = re.search(r"'([^']+)'", address_result).group(1)

    def call(bus, path, method, *args):
        return subprocess.check_output(["gdbus", "call", "--address", address, "--dest", bus, "--object-path", path, "--method", method, "--", *map(str, args)], text=True, timeout=3)

    def children(bus, path):
        result = call(bus, path, "org.a11y.atspi.Accessible.GetChildren")
        return re.findall(r"\('([^']+)', (?:objectpath )?'([^']+)'\)", result)

    baseline = set(children("org.a11y.atspi.Registry", "/org/a11y/atspi/accessible/root"))
    child = subprocess.Popen([helper], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    actions = queue.Queue(maxsize=16)

    def read_actions():
        for line in child.stdout:
            try:
                actions.put_nowait(json.loads(line))
            except (ValueError, queue.Full):
                return

    reader = threading.Thread(target=read_actions, daemon=True)
    reader.start()

    def snapshot(text):
        def node(identifier, role, label, parent=None, live=None):
            return {"id": identifier, "role": role, "label": label, "parent": parent, "enabled": True, "focused": identifier == "composer", "bounds": [0, 0, 10000, 5000], "liveText": live}
        child.stdin.write(json.dumps([node("window", "Window", "Synthetic Goya acceptance"), node("composer", "TextInput", "Synthetic message", "window", text), node("quit", "Button", "Synthetic action", "window")]) + "\n")
        child.stdin.flush()

    def wait_until(callback, seconds=8):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            value = callback()
            if value:
                return value
            if child.poll() is not None:
                raise RuntimeError("owned accessibility helper exited before acceptance")
            time.sleep(0.05)
        raise RuntimeError("owned accessibility helper did not publish expected AT-SPI state")

    def find_owned_nodes():
        roots = set(children("org.a11y.atspi.Registry", "/org/a11y/atspi/accessible/root")) - baseline
        found = {}
        pending = list(roots)
        visited = set()
        while pending and len(visited) < 128:
            bus, path = pending.pop()
            if (bus, path) in visited:
                continue
            visited.add((bus, path))
            name = call(bus, path, "org.freedesktop.DBus.Properties.Get", "org.a11y.atspi.Accessible", "Name")
            if "Synthetic message" in name:
                found["composer"] = (bus, path)
            if "Synthetic action" in name:
                found["quit"] = (bus, path)
            pending.extend(children(bus, path))
        return found if "composer" in found and "quit" in found else None

    try:
        snapshot("synthetic first")
        nodes = wait_until(find_owned_nodes)
        composer = nodes["composer"]
        assert "synthetic first" in call(*composer, "org.a11y.atspi.Text.GetText", 0, -1)
        assert "true" in call(*nodes["quit"], "org.a11y.atspi.Action.DoAction", 0)
        wait_until(lambda: any(action.get("action") == "press" and action.get("id") == "quit" for action in list(actions.queue)))
        snapshot("updated native draft")
        wait_until(lambda: "updated native draft" in call(*composer, "org.a11y.atspi.Text.GetText", 0, -1))
        assert "true" in call(*composer, "org.a11y.atspi.EditableText.SetTextContents", "screen reader edit")
        wait_until(lambda: any(action.get("action") == "setValue" and action.get("value") == "screen reader edit" for action in list(actions.queue)))
        child.stdin.close()
        assert child.wait(timeout=3) == 0
        reader.join(timeout=2)
        return {"pass": True, "checks": ["owned helper registered on AT-SPI", "live draft exported", "native semantic update visible", "button activation returned", "accessible edit returned", "owner disconnect reaped helper"]}
    finally:
        if child.poll() is None:
            child.kill()
        child.wait(timeout=3)
        reader.join(timeout=2)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--helper", required=True)
    args = parser.parse_args()
    print(json.dumps(run(args.helper), indent=2))
