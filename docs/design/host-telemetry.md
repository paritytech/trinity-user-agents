---
title: "Host Telemetry: One Table in the Core"
type: design
status: draft
author: TarikGul
created: 2026-10-29
pr: 1797
---

# Host Telemetry: One Table in the Core

![Host telemetry architecture](assets/host-telemetry.png)

## Summary

One table in the core decides which diagnostics may leave a device. Every host reads it rather than keeping its own copy. The table is exposed through a `#[uniffi::export]` function, so it reaches iOS and Android over the bindings that exist today with no new emitter and no change to the callback signature.

## Motivation

The rule for which core markers may be exported is written out by hand once per host, in `LoggerProtocol+CoreLog.swift:9-14` and `ProductTrUAPIHostBridge.kt:448-453`. Two more restatements sit in prose, in `hosts/ios/.claude/docs/code/error-handling.md:47` and `hosts/android/.claude/docs/code/naming-and-hygiene.md:240`. Nothing compares any of them.

Three things follow from that shape.

**It grows with the host count.** Two hosts mean two copies. Desktop and dot.li would make four, in three languages. Nothing in CI would notice them diverging.

**Prefix matching exports by default.** Each host matches `truapi.chain.` and three sibling prefixes, so a marker added under one of them later is exported with no review and no test change. There are 48 distinct `truapi.*` markers today, 45 of them in production.

**Severity does not survive the crossing.** `ProductTrUAPIHostBridge.kt:441` and `LoggerProtocol+CoreLog.swift:20` both collapse every allowlisted marker to info, so `truapi.tracing.error` and `truapi.tracing.warn` arrive identically and a core error cannot be alerted on. Host-originated lines keep their severity, so two lines under one tag namespace mean different things depending on who raised them.

The detail string is also built at the call site, so what a marker may carry is decided there rather than by its type. `account_handle` (`logging.rs:107-114`) exists for exactly this and shortens an account id before it is formatted; making that the only shape a detail can take is step 2 of the delivery below.

## Current state

The pipeline has two stages, and which one governs depends on who raised the line.

```
 CORE (rust)                   BOUNDARY          HOST stage 1          HOST stage 2         SENTRY
 ---------------------------   ---------------   ------------------    -----------------    ------
 warn!/info!  --> HostSinkLayer --> on_core_log --> logCoreLine --> Timber / SwiftyBeaver --> logs
  truapi.* or WARN+, capped       (2 strings)      allowlist decides   SentryLogTree /
                                                   info vs debug       HostLogSinkDestination
                                                                       tag "truapi." + INFO+

 host-originated lines (truapi.auth, truapi.chain, truapi.product, truapi.payments)
   join at stage 2 and never cross the boundary -----------------------> logs
```

For a core line, stage 1 is the decision. `logCoreLine` logs an allowlisted marker at info and everything else at debug, and every core line carries the same `truapi.core` tag, so stage 2's `priority >= Log.INFO && tag.startsWith("truapi.")` (`SentryLogTree.kt:15-16`, and `minLevel = .info` plus the same tag guard at `HostLogSinkDestination.swift:13,25`) only executes what the allowlist already chose. Delete the allowlist and every core marker exports.

For a host-originated line, stage 2 is the only gate there is. Those lines carry their own tags and their own severities, and nothing upstream filters them.

Telemetry also leaves through eight capture senders, four per platform, with seven distinct signatures between them and four unrelated protocols on iOS (`HostEventRecording`, `IssueReporting`, `HostLogSink`, `DeviceReadingsMonitoring`). The pattern is that a feature wanting telemetry writes its own sender.

**The web host has no core-log channel at all.** `on_core_log` is declared in `native/callbacks.rs:37`, inside `truapi::native`, which is `#[cfg(not(target_arch = "wasm32"))]` at `lib.rs:793-794`. There is no `onCoreLog` in any TypeScript source, and `truapi-codegen` walks the `platform` modules (`truapi-codegen/src/platform.rs:1-10`), never `native`. So Desktop and dot.li receive no core diagnostics today, under any design.

## Options

All options assume the same core-side registry: one table of markers, each with what it carries and whether it may leave the device, and a `HostSinkLayer` that forwards on an exact match rather than a prefix. They differ only in how the table reaches a host.

None of them reaches the web host for free, because there is no channel to carry it on. That is separate work under every option and is therefore not a differentiator between them.

### Option 1: convention in the marker name

Encode the class in the string, `truapi.chain.budget_reached` becoming `truapi.export.chain.budget_reached` against `truapi.local.native.push_notification`. Each host matches one prefix.

**Trade-offs.** No new machinery and a one-line host diff, but it is a convention rather than a guarantee, it renames 45 production markers, and nothing stops the next marker choosing the wrong prefix. That is the failure mode we already have, respelled.

### Option 2: carry the class over the boundary

`on_core_log` gains an argument. Hosts route on it and hold no policy.

**Trade-offs.** The decision becomes data carried with the line, and it puts the class in the signature where a host author cannot miss it. Against that, it changes a boundary signature, so `native/callbacks.rs:37`, both native packages, both app bridges on each platform and every test double update together, including three `on_core_log` impls in `native/tests.rs` alone.

### Option 3: export the table over UniFFI

The core exposes the registry as an exported function. Each host reads it once at startup and matches exactly.

**Trade-offs.** No new emitter, no boundary change, no new CI job, and the table rides `uniffi-bindgen-cli` with every other binding. The core already exports thirteen free functions this way; `parse_navigate` at `native.rs:65-68` is the same construction, a pure stateless function over an exported type, though it is called per navigation where this is read once. Nothing here is weaker than Option 2 except that a host must call the function rather than being handed its result.

### Option 4: a new codegen emitter

`truapi-codegen` emits the table as a Kotlin object and a Swift enum.

**Trade-offs.** The Swift and Kotlin bindings come from `uniffi-bindgen-cli` (`Makefile:169,315,351,363`), not from `truapi-codegen`, so this is two new emitters rather than reuse of the existing Rust and TypeScript ones. It buys nothing Option 3 does not.

## Comparison

| Axis | 1: convention | 2: carried class | 3: exported table | 4: new emitter |
|---|---|---|---|---|
| Single source of truth | No | Yes | Yes | Yes |
| Boundary change | None | UniFFI signature | None | None |
| New machinery for iOS and Android | None | None | None | 2 emitters |
| Severity preserved | No | Yes | Yes | Yes |
| Cost on the hosts that exist | 45 renames | 8 conformers plus test doubles | 1 function, 2 read sites | 2 emitters, 2 read sites |

## Approach

**Option 3, with each host reading the whole table once at startup rather than calling per line.**

It is the cheapest option on the two hosts that exist, and it scales without getting more expensive. Two hosts mean two hand-written copies today and four hosts would mean four; one table keeps that number at zero however many hosts are added. What it does not do is carry itself to Desktop and dot.li, because they have no core-log channel to carry it on. Giving them one is separate work, and when it happens it should read this table rather than start a third copy.

The question that settled the shape was whether a marker's class is fixed or varies line by line. It is fixed. The case for varying it is `truapi.ws_bridge.*`, where the detail differs every time, but the answer there is to type the detail so it cannot carry what we do not want, not to let the class float. A floating class puts the decision back at the call site, which is where it sits today and what this is meant to move.

Option 2 remains the better shape on one axis, in that the class in the signature is impossible to miss, whereas this relies on a host calling a function it could skip. Both host allowlists are deleted in the same change, so there is nothing to skip to, but that is the real argument against this and it is worth recording.

## Interface

```rust
/// What a marker carries, and whether it may leave the device.
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct Marker {
    /// Dotted target, e.g. `truapi.chain.follow_budget_reached`.
    pub target: String,
    pub class: MarkerClass,
}

#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum MarkerClass {
    /// Stays on the device.
    Local,
    /// May be shipped off-device, at the given severity.
    Export(Severity),
}

#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum Severity { Info, Warn, Error }

/// Every host-facing marker and its class. A host reads this once at startup
/// and matches exactly; a target absent from it is `Local`.
#[uniffi::export]
pub fn core_markers() -> Vec<Marker> { /* ... */ }
```

What a marker may carry is prose for reviewers and stays in the Rust registry beside each entry. It is not in the exported `Record`, because nothing on a device reads it.

An unregistered target is caught three times over, in this order:

| Where | What happens |
|---|---|
| compile time | call sites use registry constants, so an unregistered marker does not build |
| `HostSinkLayer` | a target built at runtime, as `truapi.tracing.{warn,error}` is, is refused if absent |
| host | a target absent from the table it read is treated as `Local` |

The first is the real guarantee. The other two are backstops for runtime-built targets and for a host whose bindings are older than the core.

**What this changes:** exact-match lookup replaces prefix matching, so a marker absent from the table is local; `MarkerClass::Export(Severity)` means a core error is no longer a core warning in Sentry; and both host allowlists are deleted.

The two tag filters stay. They are the only gate on host-originated lines such as `truapi.payments`, which the core table says nothing about. Deleting them would ship every host debug line on any tag.

## Trade-offs

The cost is that every new marker has to be registered, which the author cannot skip because an unregistered target does not compile. That is deliberate: it is the same friction that stops a marker shipping off-device without anyone saying what it carries.

The benefit is that the number of places the rule is written stays at one as hosts are added, and that a marker is local until someone states otherwise rather than exported until someone notices.

## Open questions

1. Does the registry belong beside the logging layer in `truapi`, or in the `truapi::platform` module with the other host-facing contracts?
2. Do events and metrics want the same treatment, or is the log channel the only one with a boundary worth policing?
3. When Desktop and dot.li get a core-log channel, does it carry the class per line (Option 2's shape) or read the table (this one)?

## Delivery

Tracked on #1796, which carries a task per host alongside the Rust ones.
