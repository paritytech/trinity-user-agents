#!/usr/bin/env python3
import os
import re
import sys
from substrateinterface import SubstrateInterface

CHAINS_FILE = "chains/src/main/java/io/paritytech/polkadotapp/chains/util/Chains.kt"
HEX_HASH_PATTERN = r"[a-fA-F0-9]+"

ENDPOINTS = {
    "UNSTABLE_PEOPLE": "https://pop-testnet.parity-lab.parity.io/9910",
    "UNSTABLE_BULLET_IN": "https://pop-testnet.parity-lab.parity.io/10000",
}


def get_genesis_hash(rpc_url: str) -> str:
    with SubstrateInterface(url=rpc_url) as substrate:
        genesis_hash = substrate.get_block_hash(0)
    return genesis_hash[2:].lower() if genesis_hash.startswith("0x") else genesis_hash.lower()


def read_chains_file() -> str:
    if not os.path.exists(CHAINS_FILE):
        raise FileNotFoundError(f"Chains file not found: {CHAINS_FILE}")

    with open(CHAINS_FILE, "r") as f:
        return f.read()


def get_current_values(content: str) -> dict:
    values = {}
    for name in ENDPOINTS.keys():
        match = re.search(rf'const val {name} = "({HEX_HASH_PATTERN})"', content)
        if match:
            values[name] = match.group(1).lower()
    return values


def apply_updates(content: str, updates: dict) -> str:
    for name, new_value in updates.items():
        content = re.sub(
            rf'(const val {name} = "){HEX_HASH_PATTERN}(")',
            rf'\g<1>{new_value}\2',
            content
        )
    return content


def main():
    content = read_chains_file()
    current = get_current_values(content)
    changes = []
    updates = {}
    errors = []

    for name, rpc_url in ENDPOINTS.items():
        current_hash = current.get(name)
        if current_hash is None:
            print(f"Warning: {name} not found in {CHAINS_FILE}", file=sys.stderr)
            continue

        try:
            live_hash = get_genesis_hash(rpc_url)

            if live_hash != current_hash:
                changes.append(f"<b>{name}</b>\n{current_hash}\n→ {live_hash}")
                updates[name] = live_hash
        except Exception as e:
            errors.append(f"{name}: {e}")

    if errors:
        for error in errors:
            print(f"Error: {error}", file=sys.stderr)
        sys.exit(1)

    github_output = os.environ.get("GITHUB_OUTPUT", "/dev/null")

    if updates:
        updated_content = apply_updates(content, updates)
        with open(CHAINS_FILE, "w") as f:
            f.write(updated_content)

        message = "<b>Genesis Hash Changed!</b>\n\n" + "\n\n".join(changes)

        with open(github_output, "a") as f:
            f.write("changed=true\n")
            f.write(f"message<<EOF\n{message}\nEOF\n")

        print("Changes detected and file updated")
    else:
        with open(github_output, "a") as f:
            f.write("changed=false\n")
        print("No changes detected")


if __name__ == "__main__":
    main()
