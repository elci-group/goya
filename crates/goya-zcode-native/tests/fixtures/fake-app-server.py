#!/usr/bin/env python3
"""Deterministic local v4 app-server for monitor acceptance; never calls a model."""

import json
import pathlib
import sys


def emit(value):
    sys.stdout.write(json.dumps(value, separators=(",", ":")) + "\n")
    sys.stdout.flush()


root = pathlib.Path(__file__).resolve().parents[4]
cases = json.loads((root / "fixtures/zcode-v4/cases.json").read_text(encoding="utf-8"))
fixtures = {case["id"]: case["value"] for case in cases}
log_path = pathlib.Path.cwd() / ".fake-app-server.log"
connection_topics = {}

for line in sys.stdin:
    request = json.loads(line)
    method = request.get("method")
    params = request.get("params", {})
    with log_path.open("a", encoding="utf-8") as log:
        log.write(json.dumps(request, separators=(",", ":")) + "\n")

    if method == "v4/conversation/subscribe":
        assert params.get("clientMode") == "desktop-continuous"
        assert params.get("connectionId")
        assert set(params["workspace"]) == {"workspacePath", "workspaceKey"}
        topic = params["topic"]
        subscription_id = "fake-index-sub" if topic.startswith("sessions-index/") else "fake-conversation-sub"
        connection_topics[topic] = subscription_id
        emit({"id": request["id"], "result": {"ack": {"subscriptionId": subscription_id, "mode": "snapshot", "logEpoch": "synthetic-index-epoch" if topic.startswith("sessions-index/") else "synthetic-epoch"}}})
        if topic.startswith("sessions-index/"):
            snapshot = dict(fixtures["index.snapshot"])
            snapshot["workspaceId"] = topic.removeprefix("sessions-index/")
            snapshot["sessions"] = [dict(item, workspaceId=snapshot["workspaceId"]) for item in snapshot["sessions"]]
            payload = {"kind": "snapshot", "snapshot": snapshot}
            to_seq = 0
        else:
            snapshot = dict(fixtures["conversation.snapshot"])
            snapshot["sessionId"] = topic.removeprefix("conversation/")
            snapshot["rows"] = dict(snapshot["rows"])
            snapshot["rows"]["window"] = [fixtures["row.user"]]
            snapshot["rows"]["totalCount"] = 1
            snapshot["rows"]["firstRowId"] = 1
            payload = {"kind": "snapshot", "snapshot": snapshot}
            to_seq = snapshot["seq"]
        frame = {"topic": topic, "subscriptionId": subscription_id, "fromSeq": 0, "toSeq": to_seq, "sentAt": 0, "payload": payload}
        emit({"method": "v4/conversation/frame", "params": {"wireVersion": 3, "kind": "complete", "deliveryKind": "initial", "logicalFrameId": f"initial-{subscription_id}", "logicalFrameOrdinal": 1, "topic": topic, "subscriptionId": subscription_id, "frame": frame}})
    elif method == "v4/conversation/rowsRange":
        assert set(params) <= {"sessionId", "beforeRowId", "limit"}
        emit({"id": request["id"], "result": {"rows": [], "atSeq": fixtures["conversation.snapshot"]["seq"], "atRevision": fixtures["conversation.snapshot"]["revision"], "atLogEpoch": "synthetic-epoch", "hasMore": False}})
    elif method == "v4/command":
        assert params.get("type") == "sendText"
        assert params.get("commandId") and params.get("clientId")
        assert params.get("sessionId") == "synthetic-session"
        assert params.get("baseRevision") == 1
        assert params.get("payload", {}).get("text") == "synthetic input"
        # Simulate a lost ACK after admission by withholding the response.
        # The monitor must wait for its normal timeout, query the same key,
        # and must not resend the input.
    elif method == "v4/commands/query":
        commands = params.get("commands", [])
        assert len(commands) == 1 and commands[0]["sessionId"] == "synthetic-session"
        command_id = commands[0]["commandId"]
        ack = dict(fixtures["command.accepted"], commandId=command_id)
        emit({"id": request["id"], "result": {"results": [{"key": commands[0], "result": ack}]}})
    elif method in ("v4/conversation/unsubscribe", "v4/connection/flow"):
        emit({"id": request["id"], "result": {}})
    else:
        emit({"id": request["id"], "error": {"code": -32601, "message": f"unsupported synthetic method: {method}"}})
