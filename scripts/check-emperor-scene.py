#!/usr/bin/env python3
"""Qualify an Emperor scene App (emperor.linux.scene/0.0.1) built from
apps/pairs.gscene, headless: no compositor is needed.

usage: check-emperor-scene.py APP SCENE WORKDIR

It plays Goya Pairs with the remote-style keys the App's --exercise mode
accepts (arrows, OK), reading the deal from the scene's accessible labels,
and checks the game state, the D9 announcements, the D8 record in
$XDG_STATE_HOME/goya, restore in a fresh process, refusal to overwrite a
record that failed to restore (D4), and the AccessKit tree. It renders a
mid-game frame to WORKDIR/midgame.png. Prints PASS/FAIL lines and exits 1 on
any failure.
"""
import json, os, re, shutil, subprocess, sys

app, scene_path, work = sys.argv[1:4]
src = open(scene_path).read()
log = []


def check(name, got, want):
    log.append(("PASS " if got == want else "FAIL ") + f"{name}: got {got!r}, want {want!r}")


def run(state_home, *args):
    env = dict(os.environ, XDG_STATE_HOME=state_home)
    out = subprocess.run([app, *args], capture_output=True, text=True, env=env, timeout=600)
    if out.returncode != 0:
        raise SystemExit(f"App {' '.join(args)} failed: {out.stderr}")
    return out.stdout


def exercise(state_home, keys):
    return json.loads(run(state_home, "--exercise", keys))


pics = {}
for j in range(16):
    m = re.search(rf'id: "card\.{j}\.c{j}_d0_done0";[^@]*@iden {{ label: "[^:]*: ([a-z ]+), matched"', src)
    pics[j] = m.group(1)
groups = {}
for j, p in pics.items():
    groups.setdefault(p, []).append(j)


class Pad:
    """Key script from card to card: focus starts on card 0."""

    def __init__(self):
        self.at = 0
        self.keys = ""

    def go(self, j):
        r, c, tr, tc = self.at // 4, self.at % 4, j // 4, j % 4
        self.keys += ("D" * (tr - r) if tr > r else "U" * (r - tr)) + ("R" * (tc - c) if tc > c else "L" * (c - tc))
        self.at = j

    def pick(self, j):
        self.go(j)
        self.keys += "O"


state = os.path.join(work, "state")
shutil.rmtree(state, ignore_errors=True)
first = exercise(state, "O")
check("first pick turns card 1", [first["state"]["phase"], first["state"]["sel_0"]], [1, True])
tree = run(state, "--tree", "--keys", "O")
check("card 1's held variant is exposed by name", "Card 1, row 1, column 1: " + pics[0] + ", turned over" in tree, True)
check("live text is exposed as a label value", 'value=Some("Pairs: 0")' in tree, True)

pad = Pad()
pad.pick(0)
miss = next(j for j in range(1, 16) if pics[j] != pics[0])
pad.pick(miss)
r = exercise(state, pad.keys)
check("a miss shows both cards", [r["state"]["phase"], r["state"][f"sec_{miss}"], r["state"]["moves"]], [2, True, 1])
check("D9 announcement for the miss", r["announcement"], "Moves increased by 1, from 0 to 1")
pad.keys += "O"
for n, (p, (a, b)) in enumerate(groups.items()):
    pad.pick(a)
    pad.pick(b)
    if n == 0:
        r = exercise(state, pad.keys)
        check("D9 announcement for a match", r["announcement"], "Pairs increased by 1, from 0 to 1; Moves increased by 1, from 1 to 2")
    if n == 2:
        run(state, "--render", os.path.join(work, "midgame.png"), "--size", "1920x1080", "--keys", pad.keys)
r = exercise(state, pad.keys)
check("all pairs found", [r["state"]["pairs"], r["state"]["moves"]], [8, 9])
pad.keys += "RRRRO"
r = exercise(state, pad.keys)
check("Play again counts the win and deals again", [r["state"]["games"], r["state"]["deal"], r["state"]["pairs"]], [1, 1, 0])
check("D9 announcement for Play again", r["announcement"], "Pairs decreased by 8, from 8 to 0; Moves decreased by 9, from 9 to 0; Games won increased by 1, from 0 to 1")
check("no runtime errors", r["errors"], [])
record = os.path.join(state, "goya", "goya_pairs.json")
check("D8 record written to the App's own state file", open(record).read(), '{"format":"goya.state.v1","values":{"games":["int","1"]}}')
check("only that file was written", sorted(os.listdir(state)) + sorted(os.listdir(os.path.join(state, "goya"))), ["goya", "goya_pairs.json"])
r = exercise(state, "")
check("durable games restored in a fresh process", [r["state"]["games"], r["state"]["pairs"]], [1, 0])
# D4: a record that fails to restore is reported and never overwritten.
open(record, "w").write('{"format":"goya.state.v1","values":{"games":["text","x"]}}')
r = exercise(state, pad.keys)
check("retyped record is refused at restore", any("retyped" in e for e in r["errors"]), True)
check("and not overwritten", open(record).read(), '{"format":"goya.state.v1","values":{"games":["text","x"]}}')

print("\n".join(log))
fails = sum(line.startswith("FAIL") for line in log)
print(f"{len(log) - fails} PASS, {fails} FAIL")
sys.exit(1 if fails else 0)
