// The core runs in this worker. Vite resolves the core it loads to the
// signing-enabled `testing` bundle; see `vite.config.ts`.
import "@parity/truapi-host/worker-runtime";
