#!/usr/bin/env python3
"""Synthetic semantic helper; never connects to a Host or model."""
import json
import sys
for line in sys.stdin:
    json.loads(line)
    for action in [
        {"action": "press", "id": "disabled"},
        {"action": "press", "id": "missing"},
        {"action": "press", "id": "quit"},
    ]:
        print(json.dumps(action), flush=True)
