// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

import { spawn, spawnSync } from "node:child_process";
import {
  createWriteStream,
  existsSync,
  readdirSync,
  writeFileSync,
} from "node:fs";
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
/** A dump of a busy emulator can take more than 20 s; a slow dump is not a broken adb server. */
const UI_DUMP_TIMEOUT_MS = 90_000;
/** How long a logcat stream waits before it connects again after its adb process ended. */
const LOGCAT_RECONNECT_MS = 1_000;
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
const restartListeners = new Set();

/**
 * Call `listener` after each restart of the adb server. A restart drops the
 * `adb reverse` ports, so a caller that maps a port maps it again here.
 */
export function onAdbServerRestart(listener) {
  restartListeners.add(listener);
  return () => restartListeners.delete(listener);
}

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
  for (const listener of restartListeners) {
    try {
      listener();
    } catch {
      // A listener that fails does not stop the others.
    }
  }
  return true;
}

function adb(args, { serial, timeoutMs = DEFAULT_ADB_TIMEOUT_MS, restartOnTimeout = true } = {}) {
  const full = serial ? ["-s", serial, ...args] : args;
  let result = runAdbOnce(full, timeoutMs);
  if (result.timedOut && !restartOnTimeout) return result;
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

function startEmulator(avd, port) {
  const available = listAvds();
  if (!available.includes(avd)) {
    throw new Error(
      `AVD not found: ${avd}. Available: ${available.join(", ") || "none"}.\n` +
        `Create it with: avdmanager create avd -n ${avd} -k "system-images;android-36;google_apis;${systemImageAbi()}"`,
    );
  }
  const child = spawn(
    emulatorPath(),
    ["-avd", avd, ...(port ? ["-port", String(port)] : []), ...EMULATOR_HEADLESS_ARGS],
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

/**
 * The serial of a booted device. With no device online, boot `avd` headless.
 * A `port` pins the console port of the emulator, so its serial is
 * `emulator-<port>`, and two emulators can run beside each other.
 */
export async function ensureEmulator({
  avd = DEFAULT_AVD,
  serial: requested,
  port,
  timeoutMs = BOOT_TIMEOUT_MS,
  log = () => {},
} = {}) {
  const serial = requested ?? (port ? emulatorSerial(port) : undefined);
  let device = onlineDevice(serial);
  if (!device) {
    log(`no device online; booting AVD ${avd} headless`);
    startEmulator(avd, port);
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

/** The adb serial of the emulator whose console port is `port`. */
export function emulatorSerial(port) {
  return `emulator-${port}`;
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

/**
 * Copy a file of the host to the device, and add it to the media store, so
 * that the document picker lists it.
 */
export function pushFile(serial, source, destination) {
  const pushed = adbOrThrow(["push", source, destination], { serial, timeoutMs: INSTALL_TIMEOUT_MS });
  adbShell(serial, ["content", "call", "--uri", "content://media/", "--method", "scan_volume", "--arg", "external_primary"]);
  return pushed;
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

/** True when an emulator or device is online, before the launcher boots one. */
export function isDeviceOnline(serial) {
  return onlineDevice(serial) !== undefined;
}

/** Stop an emulator that the launcher booted. */
export function killEmulator(serial) {
  return adb(["emu", "kill"], { serial });
}

/**
 * The package name of an APK, from `aapt2` of the newest installed build
 * tools, or undefined when no `aapt2` is installed.
 */
export function apkPackageName(apk) {
  const buildTools = resolve(androidSdkRoot(), "build-tools");
  if (!existsSync(buildTools)) {
    return undefined;
  }
  const versions = readdirSync(buildTools)
    .filter((name) => existsSync(resolve(buildTools, name, "aapt2")))
    .sort((a, b) => a.localeCompare(b, undefined, { numeric: true }));
  const latest = versions.at(-1);
  if (!latest) {
    return undefined;
  }
  const result = spawnSync(
    resolve(buildTools, latest, "aapt2"),
    ["dump", "packagename", apk],
    { encoding: "utf8", timeout: DEFAULT_ADB_TIMEOUT_MS },
  );
  const name = result.status === 0 ? result.stdout.trim() : "";
  return /^[A-Za-z0-9_.]+$/.test(name) ? name : undefined;
}

/** Run a shell command on the device; the parts are quoted for the shell. */
export function shell(serial, parts, options = {}) {
  const result = adbShell(serial, parts, options);
  return { status: result.status, stdout: result.stdout, stderr: result.stderr };
}

/** Run a shell command on the device as root, through `su`. */
export function rootShell(serial, command) {
  return shell(serial, ["su", "0", "sh", "-c", command]);
}

/** The rows of a query against an app database, as objects. */
export function runAsSqliteJson(serial, packageName, database, sql) {
  const result = adbShell(serial, [
    "run-as",
    packageName,
    "sqlite3",
    "-json",
    database,
    sql,
  ]);
  const output = `${result.stdout}${result.stderr}`;
  if (result.status !== 0 || /run-as:|Error:|not debuggable|unknown package/i.test(output)) {
    throw new Error(
      `run-as ${packageName} sqlite3 ${database} failed:\n${output.trim()}`,
    );
  }
  const text = result.stdout.trim();
  return text ? JSON.parse(text) : [];
}

/** The UI hierarchy of the screen, as the XML that `uiautomator dump` writes. */
export function uiDump(serial) {
  const result = adb(["exec-out", "uiautomator", "dump", "/dev/tty"], {
    serial,
    timeoutMs: UI_DUMP_TIMEOUT_MS,
    restartOnTimeout: false,
  });
  if (result.timedOut) {
    // A dump that does not end keeps the accessibility connection, and the next dump cannot start.
    adbShell(serial, ["pkill", "-f", "uiautomator"]);
  }
  const text = result.stdout ?? "";
  const end = text.lastIndexOf("</hierarchy>");
  return end < 0 ? "" : text.slice(text.indexOf("<?xml"), end + "</hierarchy>".length);
}

export function inputTap(serial, x, y) {
  return adbShell(serial, ["input", "tap", String(Math.round(x)), String(Math.round(y))]);
}

export function inputLongPress(serial, x, y, durationMs = 900) {
  const [px, py] = [String(Math.round(x)), String(Math.round(y))];
  return adbShell(serial, ["input", "swipe", px, py, px, py, String(durationMs)]);
}

export function inputSwipe(serial, from, to, durationMs = 300) {
  return adbShell(serial, [
    "input",
    "swipe",
    ...[from.x, from.y, to.x, to.y].map((value) => String(Math.round(value))),
    String(durationMs),
  ]);
}

export function inputKeyEvent(serial, ...keyCodes) {
  return adbShell(serial, ["input", "keyevent", ...keyCodes.map(String)]);
}

export function inputKeyCombination(serial, ...keyCodes) {
  return adbShell(serial, ["input", "keycombination", ...keyCodes.map(String)]);
}

/**
 * Write the whole logcat of the device to `destination` from now on, and
 * keep its lines in memory. `stop` ends the stream.
 */
/** The `-T` time of a `threadtime` logcat line, or undefined. */
export function logcatResumeTime(line) {
  return /^(\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3})\s/.exec(line ?? "")?.[1];
}

/**
 * Record the whole logcat of the device into `destination` and into `lines`.
 * When the adb process ends before `stop()`, for example because the adb
 * server restarted, the stream connects again from the time of its last line
 * and leaves out the lines that it has already.
 */
export function streamLogcat(serial, destination) {
  const file = createWriteStream(destination);
  const lines = [];
  let stopped = false;
  let child;

  const connect = () => {
    const since = logcatResumeTime(lines.at(-1));
    const recent = since ? new Set(lines.slice(-2000)) : undefined;
    let partial = "";
    child = spawn(adbPath(), ["-s", serial, "logcat", "-v", "threadtime", ...(since ? ["-T", since] : [])], {
      stdio: ["ignore", "pipe", "ignore"],
    });
    child.stdout.on("data", (chunk) => {
      // The adb process can still send data after stop(), which ends the file.
      if (stopped) return;
      const text = partial + chunk.toString("utf8");
      const parts = text.split(/\r?\n/);
      partial = parts.pop() ?? "";
      const fresh = recent ? parts.filter((line) => !recent.has(line)) : parts;
      if (fresh.length > 0) file.write(`${fresh.join("\n")}\n`);
      lines.push(...fresh);
    });
    child.on("exit", () => {
      if (!stopped) setTimeout(() => !stopped && connect(), LOGCAT_RECONNECT_MS);
    });
  };
  connect();

  return {
    lines,
    stop() {
      stopped = true;
      child.kill();
      file.end();
    },
  };
}

/** Forward a host TCP port to a local abstract socket of the device. */
export function forwardAbstractSocket(serial, port, socket) {
  return adbOrThrow(["forward", `tcp:${port}`, `localabstract:${socket}`], { serial });
}

export function removeForward(serial, port) {
  return adb(["forward", "--remove", `tcp:${port}`], { serial });
}
