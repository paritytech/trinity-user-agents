import { spawn } from "node:child_process";
import { once } from "node:events";
import { randomBytes } from "node:crypto";
import { mkdtempSync, rmSync } from "node:fs";
import { createSocket } from "node:dgram";
import { tmpdir } from "node:os";
import { join } from "node:path";

// Media peers are relay-only, so these specs need a real loopback TURN relay.
// TRUAPI_MEDIA_TURNSERVER names a coturn `turnserver` binary (default: PATH).
async function freeUdpPort(): Promise<number> {
  const socket = createSocket("udp4");
  socket.bind(0, "127.0.0.1");
  await once(socket, "listening");
  const { port } = socket.address();
  socket.close();
  await once(socket, "close");
  return port;
}

export default async function globalSetup(): Promise<() => Promise<void>> {
  const binary = process.env.TRUAPI_MEDIA_TURNSERVER ?? "turnserver";
  const state = mkdtempSync(join(tmpdir(), "truapi-media-turn-"));
  const port = await freeUdpPort();
  const relayBase = 20_000 + (port % 20_000);
  const username = "media";
  const credential = randomBytes(18).toString("base64url");
  const server = spawn(
    binary,
    [
      "-n",
      "--listening-ip=127.0.0.1",
      "--relay-ip=127.0.0.1",
      `--listening-port=${port}`,
      `--min-port=${relayBase}`,
      `--max-port=${relayBase + 200}`,
      "--realm=truapi-playground.local",
      "--lt-cred-mech",
      `--user=${username}:${credential}`,
      `--userdb=${join(state, "turndb.sqlite")}`,
      `--pidfile=${join(state, "turnserver.pid")}`,
      "--fingerprint",
      "--no-tls",
      "--no-multicast-peers",
      "--allow-loopback-peers",
      "--log-file=stdout",
      "--simple-log",
    ],
    { cwd: state, stdio: ["ignore", "pipe", "pipe"] },
  );
  let log = "";
  const ready = Promise.withResolvers<void>();
  const timer = setTimeout(
    () => ready.reject(new Error(`TURN relay did not start:\n${log}`)),
    10_000,
  );
  const onData = (chunk: Buffer) => {
    log += chunk.toString();
    if (/Total auth threads/.test(log)) ready.resolve();
  };
  server.stdout.on("data", onData);
  server.stderr.on("data", onData);
  server.once("error", (error) =>
    ready.reject(new Error(`Cannot start ${binary}: ${error.message}`)),
  );
  server.once("exit", (code) =>
    ready.reject(new Error(`${binary} exited (${code}):\n${log}`)),
  );
  try {
    await ready.promise;
  } finally {
    clearTimeout(timer);
  }
  process.env.TRUAPI_MEDIA_ICE_SERVERS = JSON.stringify([
    { urls: [`turn:127.0.0.1:${port}?transport=udp`], username, credential },
  ]);
  return async () => {
    if (server.exitCode === null) {
      server.kill("SIGTERM");
      await once(server, "exit");
    }
    rmSync(state, { recursive: true, force: true });
  };
}
