import type { SandboxPolicy } from "./policy.js";

declare global {
  /** The operator's sandbox policy, injected into the loader and the worker when they are built. */
  const __SANDBOX_POLICY__: SandboxPolicy;
}
