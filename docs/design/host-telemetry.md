---
title: "Host Telemetry: One Table in the Core"
type: design
status: accepted
author: TarikGul
created: 2026-10-29
---

# Host Telemetry: One Table in the Core

![Host telemetry architecture](assets/host-telemetry.png)

## Summary

One table in the core decides which diagnostics may leave a device. Every host reads it rather than keeping its own copy. The table is exposed through a `#[uniffi::export]` function, so it reaches iOS and Android over the bindings that exist today with no new emitter and no change to the callback signature.

## Conventions

These are the standards for anything that sends telemetry from a host, and `AGENTS.md` points here for them. The rest of this document is the design they came out of.

**Logs, issues and metrics are three products, not three spellings of one.** Sentry stores and indexes a log line, a captured event and a gauge separately. A diagnostic line goes to the log sink, something that broke goes out as an event, and a measurement goes out as a gauge. Sending one as another leaves it unqueryable beside its own kind.

**A feature reports through the host's capture, never to Sentry.** An Android feature takes `HostEventCapture` and an iOS feature takes `HostEventRecording` or `IssueReporting`. A feature module that depends on the Sentry SDK is itself the defect, because it is a second place deciding what leaves the device.

**Tags are the only field a query can select on.** Whatever a search, a dashboard or an alert needs to filter by belongs in tags. Extras carry the detail you read once you have already found the event, and `setExtra` is deprecated in favour of structured contexts, so detail that belongs together belongs on a context.

**Every captured event sets a fingerprint.** An event that sets none is grouped by the stack trace Sentry attaches, which splits one event name across several groups as its frames drift. One group per event name is the default, and an event supplies its own key only where its grouping is finer than its name, as an issue does on its flow and kind.

**Severity says whether something broke.** A flow that broke carries `error` and a lifecycle step carries `info`, which is what makes `level` worth filtering on at all.

**The core's marker table decides what may leave the device.** A host matches a target against the table rather than keeping a list of its own, so the question of what is exportable is answered in one reviewable place.

**No free text in a tag.** A tag's value comes from a closed set the code already knows. `CriticalIssue` takes its flow and kind as `StaticString` so an interpolated value cannot become one, and the key a threshold diagnostic counts against stays on the device.

## Motivation

Writing the export rule once per host is the obvious shape, and it does not hold. The same rule has to exist in Swift and in Kotlin, with a restatement in the prose beside each of them, and nothing can compare the copies.

Three things follow from that.

**It grows with the host count.** Two hosts mean two copies. Desktop and dot.li make four, in three languages. Nothing in CI notices them diverging.

**Prefix matching exports by default.** Matching `truapi.chain.` and three sibling prefixes exports a marker added under one of them later, with no review and no test change. There are 48 distinct `truapi.*` markers, 45 of them in production.

**Collapsing severity at the boundary loses the alert.** A host that logs every exported marker at info makes `truapi.tracing.error` and `truapi.tracing.warn` arrive identically, so a core error cannot be alerted on. Host-originated lines keep their severity, so two lines under one tag namespace mean different things depending on who raised them.

The detail string is also built at the call site, so what a marker may carry is decided there rather than by its type. `logging::Account` and `logging::HostText` cover the two values that must not be written out whole, an account and a string a host handed the core, and both redact under `Display` and `Debug` so either sigil is safe to reach for. A value that holds an account inside it redacts in its own `Debug` for the same reason.

## The pipeline

A line travels two stages, and which one governs depends on who raised it.

```
 CORE (rust)                   BOUNDARY          HOST stage 1          HOST stage 2         SENTRY
 ---------------------------   ---------------   ------------------    -----------------    ------
 warn!/info!  --> HostSinkLayer --> on_core_log --> logCoreLine --> Timber / SwiftyBeaver --> logs
  truapi.* or WARN+, capped,      (2 strings)      the table gives     SentryLogTree /
  unlisted target refused                          the severity        HostLogSinkDestination
                                                                       tag "truapi." + INFO+

 host-originated lines (truapi.auth, truapi.chain, truapi.product, truapi.payments)
   join at stage 2 and never cross the boundary -----------------------> logs
```

For a core line the table is the decision. `logCoreLine` logs a marker at the severity the table gives it, and a marker the table does not list at debug, and every core line carries the same `truapi.core` tag, so stage 2 only executes what the table already chose.

## What this does not touch

For a host-originated line, stage 2 is the only gate there is. Those lines carry their own tags and their own severities, and nothing upstream filters them, which is why the two tag filters stay (`SentryLogTree.kt:15-16`, and `minLevel = .info` plus the same tag guard at `HostLogSinkDestination.swift:13,25`).

Telemetry also leaves through seven capture senders, three on Android and four on iOS, with four unrelated protocols on iOS (`HostEventRecording`, `IssueReporting`, `HostLogSink`, `DeviceReadingsMonitoring`). The pattern is that a feature wanting telemetry writes its own sender.

**The web host has no core-log channel at all.** `on_core_log` is declared in `native/callbacks.rs:37`, inside `truapi::native`, which is `#[cfg(not(target_arch = "wasm32"))]` at `lib.rs:898`. There is no `onCoreLog` in any TypeScript source, and `truapi-codegen` walks the `platform` modules, never `native`. So Desktop and dot.li receive no core diagnostics, under any design.

## Approach

The core owns a table of every marker it may address to a host, each saying whether it may leave the device and at what severity. What its detail may hold is the doc comment beside it, read by reviewers rather than by anything on a device. It exposes that table through a `#[uniffi::export]` function, the way `parse_navigate` at `native.rs:65-68` is already exposed, so it rides `uniffi-bindgen-cli` with every other binding and needs no new emitter and no new CI job. Each host reads it once at startup and matches a target exactly, instead of keeping a list of exported prefixes of its own.

Exact matching is what makes a marker local by default. A target the table does not hold stays on the device, so a marker added later cannot ship off-device until someone says what it carries. `HostSinkLayer` applies the same rule on the way out, and refuses a host-addressed target the table does not list.

The class is a property of the marker rather than of the line. The case for varying it per line is `truapi.ws_bridge.*`, where the detail differs every time, but the answer there is to type the detail so it cannot carry what we do not want, not to let the class float. A floating class puts the decision back at the call site, which is what the table takes away.

The table reaches Desktop and dot.li as a generated TypeScript module, emitted from the same list, because UniFFI cannot reach a host that runs in a browser. Nothing there reads it yet. They still have no core-log channel, and giving them one is separate work that should read this table rather than start a third copy.

## Interface

```rust
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum MarkerClass {
    /// Stays on the device.
    Local,
    /// May be shipped off-device, at the given severity.
    Export { severity: Severity },
}

#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum Severity { Info, Warn, Error }

/// Every marker the core may address to a host, keyed by its dotted target.
/// A host reads this once and looks up each marker it is handed. A target the
/// map does not hold stays on the device.
#[uniffi::export]
pub fn core_markers() -> HashMap<String, MarkerClass> { /* ... */ }
```

The map is the lookup itself rather than a list a host has to index, so the bindings carry no mirror of the registry struct: only the two enums cross, as `[String: MarkerClass]` in Swift and `Map<String, MarkerClass>` in Kotlin.

The web hosts read the same table as `CORE_MARKERS`, a `ReadonlyMap<string, MarkerClass>` that `@parity/truapi-host/markers` exports. It is its own entry point so a package that decides what to forward depends on the table alone and not on the host runtime. `truapi-codegen` links against the registry and writes it out, so the UniFFI surface and the TypeScript one are two spellings of one list.

What a marker may carry is prose for reviewers. It stays in the Rust registry as the doc comment on each entry, and does not cross, because nothing on a device reads it.

An unregistered target is caught three times over, in this order:

| Where | What happens |
|---|---|
| compile time | every call site names a table constant, so a misspelled marker does not build |
| `HostSinkLayer` | a target the table does not list is refused, a literal or a runtime-built one alike |
| host | a target absent from the table it read is treated as `Local` |

The layer is what actually enforces it. The constants make a typo a build failure rather than a silent drop, and the host check is the backstop for bindings older than the core.

**What the table decides.** A marker absent from it is local, because lookup is exact. `MarkerClass::Export { severity }` is what carries a core error to Sentry as an error rather than as info. The rule exists once, in the core, whatever the host count.

The two tag filters stay. They are the only gate on host-originated lines such as `truapi.payments`, which the core table says nothing about. Deleting them would ship every host debug line on any tag.

## Trade-offs

The cost is that every new marker has to be registered, which the author cannot skip because an unregistered target does not compile. That is deliberate: it is the same friction that stops a marker shipping off-device without anyone saying what it carries.

A host has to call the function to hold the table at all, so a host that never calls it holds no policy. Carrying the class on the callback itself would make that impossible to miss, at the cost of a boundary signature every conformer and test double would follow. There is no per-host fallback underneath it, which is the real weakness here and worth stating.

The benefit is that the number of places the rule is written stays at one as hosts are added, and that a marker is local until someone states otherwise rather than exported until someone notices.

## Open questions

1. Does the registry belong beside the logging layer in `truapi`, or in the `truapi::platform` module with the other host-facing contracts?
2. Do events and metrics want the same treatment, or is the log channel the only one with a boundary worth policing?
3. When Desktop and dot.li get a core-log channel, does it carry the class on each line or read the table the way the native hosts do?

## Delivery

The Rust table lands first, then iOS and Android read it and their own lists go with it. Desktop and dot.li read the same table once they have a core-log channel to carry it on. Tracked on #1796, which carries a task per host alongside the Rust ones.
