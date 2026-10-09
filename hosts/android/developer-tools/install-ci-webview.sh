#!/usr/bin/env bash
# Test-device provisioning only: never install this frozen AOSP fixture on a user's device.
set -euo pipefail

if [[ $# != 1 ]]; then
  echo "Usage: bash $0 <owned-api30-emulator-serial>" >&2
  exit 1
fi
adb=("${ANDROID_HOME:?Set ANDROID_HOME}/platform-tools/adb" -s "$1")
if [[ "$("${adb[@]}" shell getprop ro.kernel.qemu | tr -d '\r')" != 1 ||
      "$("${adb[@]}" shell getprop ro.build.version.sdk | tr -d '\r')" != 30 ||
      "$("${adb[@]}" shell getprop ro.build.type | tr -d '\r')" != userdebug ||
      "$("${adb[@]}" shell getprop ro.product.cpu.abi | tr -d '\r')" != x86_64 ]]; then
  echo 'This fixture requires an owned API 30 x86_64 userdebug emulator.' >&2
  exit 1
fi

# Official AOSP prebuilt, not an APK mirror or a moving latest-channel URL:
# https://android.googlesource.com/platform/external/chromium-webview/+/27ac2bdb9a1a4e55dc65bb6cdc752dbfdea3b9f4/
# com.android.webview 128.0.6613.88 (661308807), minSdk 26, targetSdk 35.
# APK signing certificate SHA-256 (AOSP development key, NOT Google's Play key):
# 32a2fc74d731105859e5a85df16d95f102d85b22099b8064c5d8915c61dad1e0
# The immutable APK digest below pins the signed bytes, including that certificate.
readonly revision=27ac2bdb9a1a4e55dc65bb6cdc752dbfdea3b9f4
readonly version=128.0.6613.88
readonly sha256=deb1987501d40c4c326697be8fdd7733abdf2e0ab7fb6e028205b0ff786876e5
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
curl --fail --location --retry 3 --connect-timeout 30 --max-time 300 \
  "https://android.googlesource.com/platform/external/chromium-webview/+/$revision/$version/x86_64/webview.apk?format=TEXT" \
  --output "$work/webview.apk.base64"
base64 --decode "$work/webview.apk.base64" > "$work/webview.apk"
printf '%s  %s\n' "$sha256" "$work/webview.apk" | sha256sum --check --strict

# Google APIs userdebug images allow com.android.webview alongside their stock
# Google provider. No root, system-image replacement, signature bypass, or
# production JavaScript fallback is needed. Selection alone can exit zero even
# when Android rejects a provider, so verify the actual selected package too.
"${adb[@]}" install --no-incremental -r "$work/webview.apk"
"${adb[@]}" shell cmd webviewupdate set-webview-implementation com.android.webview
state=$("${adb[@]}" shell dumpsys webviewupdate)
printf '%s\n' "$state"
if [[ "$state" != *"Current WebView package (name, version): (com.android.webview, $version)"* ]]; then
  echo 'The pinned WebView was not selected; refusing to run against another provider.' >&2
  exit 1
fi
