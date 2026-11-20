/// <reference path="../runner.ts" />
// The funding battery's provider worker.
//
// The battery names this script as the provider's worker in
// `TRUAPI_FUNDING_WORKERS`, and the signing host launches it as its own
// process, served as a Worker execution, when the core asks for the
// provider's worker, and kills it when the core lets it go. It serves the
// sessions the core assigns and records what it was asked and did in
// `TRUAPI_FUNDING_WORKER_LOG`, which the product's cases read. It runs until
// the host stops it or drops its connection.
import { ScriptedProvider } from "../funding-provider-e2e.ts";

const journalPath = process.env.TRUAPI_FUNDING_WORKER_LOG;
const fundingLog = process.env.TRUAPI_FUNDING_LOG;
if (!journalPath || !fundingLog) {
  throw new Error(
    "the funding worker needs TRUAPI_FUNDING_WORKER_LOG and TRUAPI_FUNDING_LOG",
  );
}

/** The host transport the runner injects, as a host-embedded page sees it. */
interface HostLink {
  subscribeConnectionStatus(
    listener: (status: "disconnected" | "connecting" | "connected") => void,
  ): () => void;
}
const link = (globalThis as { __HOST_API_TRANSPORT__?: HostLink })
  .__HOST_API_TRANSPORT__;
if (!link) throw new Error("the runner exposes no host connection");

new ScriptedProvider(truapi, journalPath, fundingLog);
await new Promise<void>((resolve) =>
  link.subscribeConnectionStatus((status) => {
    if (status === "disconnected") resolve();
  }),
);
