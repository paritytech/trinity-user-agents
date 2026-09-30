#!/usr/bin/env python3
"""Helper for tailscale-serve.sh: reads `tailscale serve status --json` on stdin
and keeps the record of which routes the script made. Never runs tailscale.

The record is a JSON file: {"owned": {"<port>": {"host": "<dns name>", "backend":
"<url>"}}, "snapshot": <Serve config before the first `up`>}. A route is ours only
while its port is recorded and the live route is still exactly the one `up`
made: same host name, same port, same single proxy handler.
"""
import json
import os
import re
import sys
import tempfile


def die(message):
    print(f"error: {message}", file=sys.stderr)
    sys.exit(1)


def load(path):
    if not os.path.exists(path):
        return None
    try:
        with open(path) as handle:
            state = json.load(handle)
        assert "snapshot" in state
        for port, route in state["owned"].items():
            assert re.fullmatch(r"[0-9]+", port)
            assert isinstance(route["host"], str) and route["host"]
            assert isinstance(route["backend"], str) and route["backend"]
    except Exception:
        die(f"{path} is not a valid record from this script (or from an older version); "
            "no route was touched. Check `tailscale serve status`, then delete the file")
    return state


def save(path, state):
    fd, temp = tempfile.mkstemp(dir=os.path.dirname(os.path.abspath(path)))
    with os.fdopen(fd, "w") as handle:
        json.dump(state, handle, indent=2)
    os.replace(temp, path)


def read_status():
    text = sys.stdin.read()
    try:
        return json.loads(text) if text.strip() else {}
    except ValueError:
        die("could not parse the Serve config")


def verdict(status, port, host, backend):
    """free, match (exactly the route `up` makes), funnel or conflict, plus detail."""
    webs = {k: v for k, v in (status.get("Web") or {}).items() if k.rsplit(":", 1)[-1] == port}
    key = f"{host}:{port}"
    tcp = (status.get("TCP") or {}).get(port)
    funnel = any(v for k, v in (status.get("AllowFunnel") or {}).items() if k.rsplit(":", 1)[-1] == port)
    if funnel:
        return "funnel", "Funnel is on for it, so it is reachable from the internet"
    if tcp is None and not webs:
        return "free", ""
    route = {"Handlers": {"/": {"Proxy": backend}}}
    if tcp == {"HTTPS": True} and webs == {key: route}:
        return "match", ""
    return "conflict", "already in use by something else (see `tailscale serve status`)"


def classify(state_path, host, backend, ports):
    state = load(state_path)
    status = read_status()
    for port in ports:
        kind, detail = verdict(status, port, host, backend)
        if kind == "match":
            mine = state and state["owned"].get(port) == {"host": host, "backend": backend}
            kind = "owned" if mine else "existing"
        print(port, kind, detail)


def init(path):
    if load(path) is None:
        save(path, {"owned": {}, "snapshot": read_status()})


def own(path, port, host, backend):
    state = load(path)
    state["owned"][port] = {"host": host, "backend": backend}
    save(path, state)


def settle(path, port):
    """remove: still ours, caller should switch it off. Otherwise stop recording it."""
    state = load(path)
    route = state["owned"][port]
    kind, detail = verdict(read_status(), port, route["host"], route["backend"])
    answer = {"match": "remove", "free": "gone"}.get(kind, "changed")
    if answer != "remove":
        del state["owned"][port]
        save(path, state)
    print(answer, detail)


def finish(path):
    """Delete the record once it owns nothing, so the next `up` takes a new snapshot."""
    state = load(path)
    if state is not None and not state["owned"]:
        os.unlink(path)


def ports(path):
    print(*sorted(((load(path) or {"owned": {}})["owned"]), key=int), sep="\n")


def summary(path):
    owned = (load(path) or {"owned": {}})["owned"]
    print("owned by this script:", ", ".join(f"{r['host']}:{p} -> {r['backend']}" for p, r in sorted(owned.items(), key=lambda i: int(i[0]))) or "nothing")


if __name__ == "__main__":
    command, args = sys.argv[1], sys.argv[2:]
    if command == "classify":
        classify(args[0], args[1], args[2], args[3:])
    else:
        {"init": init, "own": own, "settle": settle, "finish": finish, "ports": ports, "summary": summary}[command](*args)
