// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

import { readFileSync, statSync } from "node:fs";
import { createServer } from "node:http";
import { extname, resolve, sep } from "node:path";

export function isLoopback(url) {
  return ["localhost", "127.0.0.1", "::1", "[::1]"].includes(url.hostname);
}

export async function startProductServer(urlString, root, appMarker) {
  const url = new URL(urlString);
  if (!isLoopback(url)) {
    throw new Error(
      `Product URL must be loopback for this E2E test: ${urlString}`,
    );
  }

  try {
    const response = await fetch(urlString);
    if (response.ok && (await response.text()).includes(appMarker)) {
      return null;
    }
    throw new Error(`${urlString} is serving a different application`);
  } catch (error) {
    if (
      error instanceof Error &&
      error.message.includes("different application")
    ) {
      throw error;
    }
  }

  const server = createServer((request, response) => {
    try {
      const pathname = decodeURIComponent(
        new URL(request.url ?? "/", urlString).pathname,
      );
      let file = resolve(root, `.${pathname}`);
      if (file !== root && !file.startsWith(`${root}${sep}`)) {
        response.writeHead(403).end();
        return;
      }
      if (statSync(file).isDirectory()) {
        file = resolve(file, "index.html");
      }
      response.setHeader("Content-Type", contentType(file));
      const content = readFileSync(file);
      response.end(content);
    } catch {
      response.writeHead(404).end();
    }
  });
  await new Promise((resolveListen, reject) => {
    server.once("error", reject);
    server.listen(Number(url.port || 80), url.hostname, resolveListen);
  });
  return server;
}

function contentType(file) {
  switch (extname(file)) {
    case ".html":
      return "text/html; charset=utf-8";
    case ".js":
      return "text/javascript; charset=utf-8";
    case ".css":
      return "text/css; charset=utf-8";
    case ".json":
      return "application/json";
    case ".png":
      return "image/png";
    case ".svg":
      return "image/svg+xml";
    default:
      return "application/octet-stream";
  }
}
