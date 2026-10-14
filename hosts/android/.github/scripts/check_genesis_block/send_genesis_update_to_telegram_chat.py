#!/usr/bin/env python3
import argparse
import json
import os
import sys
import urllib.parse
import urllib.request

TELEGRAM_BOT_TOKEN = os.environ["GENESIS_MONITOR_BOT_TOKEN"]
TELEGRAM_CHAT_ID = os.environ["GENESIS_MONITOR_CHAT_ID"]

REQUEST_TIMEOUT = 30


def send_telegram_alert(message: str, pr_url: str = None):
    if pr_url:
        message += f"\n\n{pr_url}"

    url = f"https://api.telegram.org/bot{TELEGRAM_BOT_TOKEN}/sendMessage"
    data = urllib.parse.urlencode({
        "chat_id": TELEGRAM_CHAT_ID,
        "parse_mode": "HTML",
        "text": message
    }).encode()

    req = urllib.request.Request(url, data=data)
    with urllib.request.urlopen(req, timeout=REQUEST_TIMEOUT) as resp:
        result = json.loads(resp.read())
        if not result.get("ok"):
            raise RuntimeError(f"Telegram API error: {result}")

    print("Telegram alert sent")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--message", required=True, help="Message to send")
    parser.add_argument("--pr-url", help="GitHub PR URL to include")
    args = parser.parse_args()

    try:
        send_telegram_alert(args.message, args.pr_url)
    except Exception as e:
        print(f"Failed to send Telegram alert: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
