#!/usr/bin/env python3
"""A stand-in `tailscale` for the tailscale-serve.sh tests. Never touches a real node.

Keeps the Serve config in the JSON file named by FAKE_TS_CONFIG
({"node": "<dns name>", "serve": {...}}) and appends every command that changes
it, or that the script must never run, to FAKE_TS_LOG.

Failure injection (environment):
  FAKE_TS_FAIL_SERVE_PORT   `serve --https=PORT URL` exits 1 and changes nothing
  FAKE_TS_FAIL_OFF_PORT     `serve --https=PORT off` exits 1 and changes nothing
"""
import json
import os
import sys

config_path = os.environ["FAKE_TS_CONFIG"]
log_path = os.environ["FAKE_TS_LOG"]
argv = sys.argv[1:]


def log(line):
    with open(log_path, "a") as handle:
        handle.write(line + "\n")


def load():
    with open(config_path) as handle:
        return json.load(handle)


def save(config):
    with open(config_path, "w") as handle:
        json.dump(config, handle)


config = load()
node = config["node"]

if argv == ["status", "--json"]:
    print(json.dumps({"BackendState": "Running", "Self": {"DNSName": node + "."}, "CertDomains": [node]}))
elif argv == ["serve", "status", "--json"]:
    print(json.dumps(config["serve"]))
elif argv == ["serve", "status"]:
    print(json.dumps(config["serve"], indent=2))
elif len(argv) == 4 and argv[:2] == ["serve", "--bg"] and argv[2].startswith("--https="):
    port, backend = argv[2].split("=", 1)[1], argv[3]
    log(" ".join(argv))
    if os.environ.get("FAKE_TS_FAIL_SERVE_PORT") == port:
        print("fake: serve failed", file=sys.stderr)
        sys.exit(1)
    serve = config["serve"]
    serve.setdefault("TCP", {})[port] = {"HTTPS": True}
    serve.setdefault("Web", {})[f"{node}:{port}"] = {"Handlers": {"/": {"Proxy": backend}}}
    save(config)
elif len(argv) == 3 and argv[0] == "serve" and argv[1].startswith("--https=") and argv[2] == "off":
    port = argv[1].split("=", 1)[1]
    log(" ".join(argv))
    if os.environ.get("FAKE_TS_FAIL_OFF_PORT") == port:
        print("fake: off failed", file=sys.stderr)
        sys.exit(1)
    serve = config["serve"]
    web_keys = [k for k in serve.get("Web", {}) if k.rsplit(":", 1)[-1] == port]
    if not web_keys and port not in serve.get("TCP", {}):
        print("fake: handler does not exist", file=sys.stderr)
        sys.exit(1)
    for key in web_keys:
        del serve["Web"][key]
        serve.get("AllowFunnel", {}).pop(key, None)
    serve.get("TCP", {}).pop(port, None)
    save(config)
else:
    log("FORBIDDEN " + " ".join(argv))
    print("fake: unsupported command", file=sys.stderr)
    sys.exit(99)
