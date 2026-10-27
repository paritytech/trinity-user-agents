import type { Plugin } from "vite";
import { loopback } from "./chat-product-demo-plugin.js";
import {
  USERNAME_SEARCH_PATH,
  searchUsernames,
} from "./src/username-search.js";

/**
 * The identity backend's username search on the dev server's own origin, at
 * {@link USERNAME_SEARCH_PATH} (`src/username-search.ts`). The page's headers,
 * cookies included, are not passed on. Requests from or to another machine
 * are refused. A build does not include it.
 */
export function usernameSearch(): Plugin {
  let base = "/";
  return {
    name: "truapi-username-search",
    apply: "serve",
    configResolved(config) {
      base = config.base.startsWith("/") ? config.base : "/";
    },
    configureServer(server) {
      server.middlewares.use((request, response, next) => {
        const url = new URL(request.url ?? "/", "http://dev.invalid");
        if (url.pathname !== `${base}${USERNAME_SEARCH_PATH}`) return next();
        if (!loopback(request)) {
          response.statusCode = 403;
          response.end("Username search answers only on this machine.");
          return;
        }
        searchUsernames(request.method ?? "GET", url)
          .then(async (answer) => {
            response.statusCode = answer.status;
            answer.headers.forEach((value, name) =>
              response.setHeader(name, value),
            );
            response.end(Buffer.from(await answer.arrayBuffer()));
          })
          .catch((error: unknown) => {
            response.statusCode = 500;
            response.end(
              error instanceof Error ? error.message : String(error),
            );
          });
      });
    },
  };
}
