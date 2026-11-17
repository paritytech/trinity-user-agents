// The core's marker table: which log markers may leave the device, and how
// loud each one is once a host's logger has it.
//
// Its own entry point because reading telemetry policy is not running a host.
// A package that only decides what to forward imports this and nothing else,
// and never names the WASM host runtime the root entry point carries.

export * from "./generated/markers.js";
