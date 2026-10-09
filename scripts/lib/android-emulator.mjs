// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

import { spawn, spawnSync } from "node:child_process";
import { existsSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { waitFor } from "./process.mjs";

export const DEFAULT_AVD = "truapi-chat-e2e";

export const DEFAULT_PACKAGE = "com.example.polkadot.debug";

const EMULATOR_HEADLESS_ARGS = [
  "-no-window",
  "-no-audio",
  "-no-boot-anim",
  "-no-snapshot",
  "-memory",
  "4096",
  "-gpu",
  "swiftshader_indirect",
];

const DEFAULT_ADB_TIMEOUT_MS = 30_000;
const BOOT_TIMEOUT_MS = 300_000;
const INSTALL_TIMEOUT_MS = 900_000;
const ADB_MAX_BUFFER = 256 * 1024 * 1024;
const ADB_RESTART_THROTTLE_MS = 15_000;

const SDK_FALLBACKS = [
  "/opt/homebrew/share/android-commandlinetools",
  "/usr/local/share/android-commandlinetools",
  process.env.HOME ? resolve(process.env.HOME, "Library/Android/sdk") : "",
  process.env.HOME ? resolve(process.env.HOME, "Android/Sdk") : "",
].filter(Boolean);

function androidSdkRoot() {
  const configured = process.env.ANDROID_HOME ?? process.env.ANDROID_SDK_ROOT;
  if (configured) {
    if (!existsSync(configured)) {
      throw new Error(`ANDROID_HOME points at a missing directory: ${configured}`);
    }
    return configured;
  }
  const found = SDK_FALLBACKS.find((candidate) => existsSync(candidate));
  if (!found) {
    throw new Error(
      "Android SDK not found. Set ANDROID_HOME (or ANDROID_SDK_ROOT), " +
        `e.g. ${SDK_FALLBACKS[0]}`,
    );
  }
  return found;
}

function sdkTool(relative, hint) {
  const tool = resolve(androidSdkRoot(), relative);
  if (!existsSync(tool)) {
    throw new Error(`${relative} not found under the Android SDK: ${tool}\n${hint}`);
  }
  return tool;
}

function adbPath() {
  return sdkTool(
    "platform-tools/adb",
    "Install it with: sdkmanager 'platform-tools'",
  );
}

function emulatorPath() {
  return sdkTool("emulator/emulator", "Install it with: sdkmanager 'emulator'");
}

export function chatIdHex(extensionId, roomId) {
  return Buffer.from(`ChatExtension:${extensionId}:${roomId}`, "utf8")
    .toString("hex")
    .toUpperCase();
}

function parseDeviceList(output) {
  return (output ?? "")
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(
      (line) =>
        line &&
        !line.startsWith("List of devices") &&
        !line.startsWith("*") &&
        !line.startsWith("adb server"),
    )
    .map((line) => line.split(/\s+/))
    .filter((parts) => parts.length >= 2)
    .map(([serial, state]) => ({ serial, state }));
}

function selectDevice(output, requested) {
  const devices = parseDeviceList(output);
  if (requested) {
    return devices.find(
      (device) => device.serial === requested && device.state === "device",
    );
  }
  return devices.find((device) => device.state === "device");
}

const ADB_UNHEALTHY_MARKERS = [
  "device offline",
  "device still authorizing",
  "device unauthorized",
  "protocol fault",
  "error: closed",
  "cannot connect to daemon",
  "daemon not running",
  "no devices/emulators found",
  "device not found",
];

function isAdbUnhealthy({
  status,
  stdout = "",
  stderr = "",
  timedOut = false,
} = {}) {
  if (timedOut) {
    return true;
  }
  if (status === 0) {
    return false;
  }
  const text = `${stdout}\n${stderr}`.toLowerCase();
  return ADB_UNHEALTHY_MARKERS.some((marker) => text.includes(marker));
}

const LOGCAT_THREADTIME =
  /^\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}\s+\d+\s+\d+\s+([VDIWEF])\s+(\S.*?)\s*:\s?(.*)$/;

function parseLogcatLine(line) {
  const match = LOGCAT_THREADTIME.exec(line ?? "");
  if (!match) {
    return undefined;
  }
  return { level: match[1], tag: match[2], message: match[3] };
}

function findLogcatMessage(output, needle) {
  for (const line of (output ?? "").split(/\r?\n/)) {
    const entry = parseLogcatLine(line);
    if (entry?.message.includes(needle)) {
      return entry;
    }
  }
  return undefined;
}

function findLogcatError(output, tag) {
  for (const line of (output ?? "").split(/\r?\n/)) {
    const entry = parseLogcatLine(line);
    if (!entry || (tag && entry.tag !== tag)) {
      continue;
    }
    if (/^error\b/i.test(entry.message)) {
      return entry;
    }
  }
  return undefined;
}

function shellQuote(value) {
  return `'${String(value).replaceAll("'", "'\\''")}'`;
}

function shellCommand(parts) {
  return parts.map(shellQuote).join(" ");
}

function parseAvdList(output) {
  return (output ?? "")
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line && !/\s/.test(line) && !line.startsWith("INFO"));
}

export function decodeSqliteHexRows(output) {
  return (output ?? "")
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((row) => /^(?:[0-9A-Fa-f]{2})+$/.test(row))
    .map((row) => Buffer.from(row, "hex").toString("utf8"));
}

function parseBroadcastResult(output) {
  return (output ?? "")
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find((line) =>
      /^(Error|Exception|java\.lang\.|Bad component)|does not exist|Unable to/i.test(
        line,
      ),
    );
}

let lastAdbRestart = 0;

function runAdbOnce(args, timeoutMs) {
  const result = spawnSync(adbPath(), args, {
    encoding: "utf8",
    timeout: timeoutMs,
    maxBuffer: ADB_MAX_BUFFER,
  });
  const timedOut =
    result.error?.code === "ETIMEDOUT" ||
    (result.signal !== null && result.signal !== undefined);
  return {
    status: result.status,
    stdout: result.stdout ?? "",
    stderr: result.stderr ?? "",
    timedOut,
  };
}

function restartAdbServer() {
  const now = Date.now();
  if (now - lastAdbRestart < ADB_RESTART_THROTTLE_MS) {
    return false;
  }
  lastAdbRestart = now;
  for (const command of ["kill-server", "start-server"]) {
    spawnSync(adbPath(), [command], {
      encoding: "utf8",
      timeout: DEFAULT_ADB_TIMEOUT_MS,
    });
  }
  return true;
}

function adb(args, { serial, timeoutMs = DEFAULT_ADB_TIMEOUT_MS } = {}) {
  const full = serial ? ["-s", serial, ...args] : args;
  let result = runAdbOnce(full, timeoutMs);
  if (isAdbUnhealthy(result) && restartAdbServer()) {
    result = runAdbOnce(full, timeoutMs);
  }
  return result;
}

function adbOrThrow(args, options = {}) {
  const result = adb(args, options);
  if (result.status !== 0) {
    const reason = result.timedOut
      ? `timed out after ${options.timeoutMs ?? DEFAULT_ADB_TIMEOUT_MS}ms`
      : `exit ${result.status}`;
    const detail = `${result.stderr}`.trim() || `${result.stdout}`.trim();
    throw new Error(`adb ${args.join(" ")} failed (${reason})${detail ? `: ${detail}` : ""}`);
  }
  return result.stdout;
}

function adbShell(serial, parts, options = {}) {
  return adb(["shell", shellCommand(parts)], { serial, ...options });
}

function onlineDevice(requested) {
  const result = adb(["devices"]);
  if (result.status !== 0) {
    return undefined;
  }
  return selectDevice(result.stdout, requested);
}

function listAvds() {
  const result = spawnSync(emulatorPath(), ["-list-avds"], {
    encoding: "utf8",
    timeout: DEFAULT_ADB_TIMEOUT_MS,
  });
  return parseAvdList(result.stdout);
}

function systemImageAbi() {
  return process.arch === "arm64" ? "arm64-v8a" : "x86_64";
}

function startEmulator(avd) {
  const available = listAvds();
  if (!available.includes(avd)) {
    throw new Error(
      `AVD not found: ${avd}. Available: ${available.join(", ") || "none"}.\n` +
        `Create it with: avdmanager create avd -n ${avd} -k "system-images;android-36;google_apis;${systemImageAbi()}"`,
    );
  }
  const child = spawn(
    emulatorPath(),
    ["-avd", avd, ...EMULATOR_HEADLESS_ARGS],
    { detached: true, stdio: "ignore" },
  );
  child.unref();
  return child;
}

function waitForBootCompleted(serial, timeoutMs) {
  return waitFor(
    () => {
      const result = adbShell(serial, ["getprop", "sys.boot_completed"]);
      return result.status === 0 && result.stdout.trim() === "1";
    },
    {
      timeoutMs,
      intervalMs: 2_000,
      message: () =>
        `Timed out waiting for sys.boot_completed=1 on ${serial} after ${timeoutMs}ms`,
    },
  );
}

export async function ensureEmulator({
  avd = DEFAULT_AVD,
  serial,
  timeoutMs = BOOT_TIMEOUT_MS,
  log = () => {},
} = {}) {
  let device = onlineDevice(serial);
  if (!device) {
    log(`no device online; booting AVD ${avd} headless`);
    startEmulator(avd);
    device = await waitFor(() => onlineDevice(serial), {
      timeoutMs,
      intervalMs: 2_000,
      message: () =>
        `Timed out waiting for AVD ${avd} to attach to adb after ${timeoutMs}ms`,
    });
  }
  await waitForBootCompleted(device.serial, timeoutMs);
  return device.serial;
}

export function installApk(serial, apk, timeoutMs = INSTALL_TIMEOUT_MS) {
  if (!existsSync(apk)) {
    throw new Error(`APK not found: ${apk}`);
  }
  const output = adbOrThrow(["install", "-r", "-t", apk], {
    serial,
    timeoutMs,
  });
  if (!/Success/.test(output)) {
    throw new Error(`adb install did not report success:\n${output.trim()}`);
  }
  return output;
}

export function clearAppData(serial, packageName) {
  const result = adbShell(serial, ["pm", "clear", packageName]);
  const output = `${result.stdout}${result.stderr}`;
  if (result.status !== 0 || !/Success/.test(output)) {
    throw new Error(`pm clear ${packageName} failed:\n${output.trim()}`);
  }
  return output;
}

export function reversePort(serial, port) {
  return adbOrThrow(["reverse", `tcp:${port}`, `tcp:${port}`], { serial });
}

export function removeReversePort(serial, port) {
  return adb(["reverse", "--remove", `tcp:${port}`], { serial });
}

export function grantPermission(serial, packageName, permission) {
  const result = adbShell(serial, ["pm", "grant", packageName, permission]);
  return { granted: result.status === 0, output: `${result.stdout}${result.stderr}`.trim() };
}

export function sendBroadcast(serial, { action, component, extras = [] }) {
  const parts = ["am", "broadcast", "--include-stopped-packages", "-a", action];
  if (component) {
    parts.push("-n", component);
  }
  for (const extra of extras) {
    parts.push(...extra);
  }
  const result = adbShell(serial, parts);
  const output = `${result.stdout}\n${result.stderr}`;
  return { error: parseBroadcastResult(output), output: output.trim() };
}

export function amStart(serial, args) {
  const result = adbShell(serial, ["am", "start", ...args]);
  const output = `${result.stdout}\n${result.stderr}`;
  if (result.status !== 0 || /Error|does not exist/i.test(output)) {
    throw new Error(`am start ${args.join(" ")} failed:\n${output.trim()}`);
  }
  return output;
}

export function clearLogcat(serial, { stopPackage } = {}) {
  if (stopPackage) {
    adbShell(serial, ["am", "force-stop", stopPackage]);
  }
  return adb(["logcat", "-c"], { serial });
}

function dumpLogcat(serial, tags) {
  const filters = tags.map((tag) => `${tag}:V`);
  const result = adb(["logcat", "-d", "-v", "threadtime", ...filters, "*:S"], {
    serial,
  });
  return result.status === 0 ? result.stdout : "";
}

export function waitForLogcatMessage(
  serial,
  { tags, marker, timeoutMs, errorTag, hint },
) {
  return waitFor(
    () => {
      const output = dumpLogcat(serial, tags);
      if (errorTag) {
        const failure = findLogcatError(output, errorTag);
        if (failure) {
          throw new Error(
            `${errorTag} reported a failure while waiting for ${JSON.stringify(marker)}: ${failure.message}`,
          );
        }
      }
      return findLogcatMessage(output, marker);
    },
    {
      timeoutMs,
      intervalMs: 1_000,
      message: () =>
        `Timed out after ${timeoutMs}ms waiting for ${JSON.stringify(marker)} on logcat tags ${tags.join(", ")}${hint ? `\n${hint}` : ""}`,
    },
  );
}

export function runAsSqlite(serial, packageName, database, sql) {
  const result = adbShell(serial, [
    "run-as",
    packageName,
    "sqlite3",
    database,
    sql,
  ]);
  const output = `${result.stdout}${result.stderr}`;
  if (result.status !== 0 || /run-as:|Error:|not debuggable|unknown package/i.test(output)) {
    throw new Error(
      `run-as ${packageName} sqlite3 ${database} failed:\n${output.trim()}`,
    );
  }
  return result.stdout;
}

export function screencap(serial, destination) {
  const result = spawnSync(adbPath(), ["-s", serial, "exec-out", "screencap", "-p"], {
    timeout: DEFAULT_ADB_TIMEOUT_MS,
    maxBuffer: ADB_MAX_BUFFER,
  });
  if (result.status !== 0 || !result.stdout?.length) {
    throw new Error(
      `adb exec-out screencap failed: ${String(result.stderr ?? "").trim() || `exit ${result.status}`}`,
    );
  }
  writeFileSync(destination, result.stdout);
  return destination;
}

export function waitForProcess(
  serial,
  packageName,
  timeoutMs = DEFAULT_ADB_TIMEOUT_MS,
) {
  return waitFor(
    () => {
      const result = adbShell(serial, ["pidof", packageName]);
      const pid = String(result.stdout ?? "").trim();
      return /^\d+$/.test(pid) ? Number(pid) : undefined;
    },
    {
      timeoutMs,
      intervalMs: 250,
      message: () => `${packageName} has no process after ${timeoutMs}ms`,
    },
  );
}
