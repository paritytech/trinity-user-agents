# Error Handling

## Core Rules

1. **Throw errors instead of force unwrapping** — prefer `guard let x = value else { throw SomeError }` over `value!`
2. **Never silently fallback** — if decoding fails, throw; don't fall back to raw bytes or defaults
3. **Use `ErrorPresentable` protocol** for user-facing error display
4. **Typed error enums** — define specific error types per domain, not generic strings

## Error Presentation Pattern

```swift
// Common/Protocols/ErrorPresentable.swift
protocol ErrorPresentable {
    func present(error: Error, from view: UIViewController)
}
```

- Interactor throws typed errors
- Presenter catches and maps to user-facing messages
- ViewController displays via ErrorPresentable

## Patterns

### Do
```swift
guard let decoded = try? decoder.decode(Model.self, from: data) else {
    throw DecodingError.invalidData
}
```

### Don't
```swift
// Force unwrap
let decoded = try! decoder.decode(Model.self, from: data)

// Silent fallback
let decoded = try? decoder.decode(Model.self, from: data) ?? defaultValue
```

## Logging

- Use `SwiftyBeaver` for logging, not `print` or `NSLog`
- Log at appropriate levels: error for failures, warning for recoverable issues, info for state changes, debug for development
- Never log PII (except public AccountId/pubkey)
- Lines written as `[truapi.<tag>] message` at info and above also go to Sentry logs in builds that link Sentry, through `HostLogSinkDestination`, the one destination that holds the `HostLogSink`. `HostLogFilter` then decides what the SDK sends: a log carrying no tag this host writes is turned away, and the rest send at most 60 lines a minute per tag, per marker on `truapi.core` and per name for a host event, with a `dropped` attribute on the first line after a minute that turned any away. The Android host sends the same lines with the same tags. Every other line stays on the device.
- On `[truapi.core]`, `logCoreLine` looks the marker up in the core's own table, read once through `coreMarkers()`, and logs it at the severity the table carries. A marker the table keeps on the device, which is every `truapi.native.` one, stays at debug, because those carry notification bodies, URLs and permission requests. A marker the table does not list at all stays at debug too, so adding one to the core without registering it costs the line rather than exporting it. The host keeps no list of its own, and the Android host reads the same table. A core event goes through `logCoreEvent` on `CoreEventLoggerProtocol`, which applies the same lookup to the same `[truapi.core] marker: key=value` line and carries the attributes as a `CoreEventContext`. `HostLogSinkDestination` hands such a line to the sink as its marker with the attributes as fields, so they arrive in Sentry beside the tag rather than inside the message.
- `JSEngineLogger` logs a product's console output untagged at its own level, so only a testnet build keeps it, in the log file, and a release build drops it. A product reports to its own Sentry project.

## Critical Issue Reporting

`IssueReporting` (IssueMonitoring package) sends events to Sentry in non-Release builds; Release is a no-op.
Sentry has event limits, so report only **critical** issues:

1. **Report only flow-breaking failures with no short-term retry** — a terminal state the user sees, or a
   retry loop that crossed its threshold. Never report a failure the code retries soon on its own.
2. **Once per occurrence** — deduplicate per item (or per period for system-wide conditions); keep retrying after
   reporting.
3. **No sensitive data** — static `flow`/`kind`, the error's type name or `ReportableError.reportCode`, numeric
   counters only. Never ids, keys, account ids or aliases, usernames, message content, hashes, derivation paths,
   `localizedDescription` or `String(describing: error)`.
4. **Keep thresholds out of business logic** — feature code only calls `recordFailure`/`recordRecovery` on injected
   `IssueDiagnostic`s. A per-feature factory owns the thresholds and returns no-op diagnostics when the build has no
   reporting SDK.

## Notification Cleanup

When cleaning up notifications, both cancel pending AND remove already-delivered notifications:

```swift
// GOOD: Cancel + remove
UNUserNotificationCenter.current().removePendingNotificationRequests(withIdentifiers: ids)
UNUserNotificationCenter.current().removeDeliveredNotifications(withIdentifiers: ids)

// BAD: Only cancelling pending (leaves delivered notifications visible)
UNUserNotificationCenter.current().removePendingNotificationRequests(withIdentifiers: ids)
```

(review: "Cancelling cancels the scheduled notification, but does not remove an already delivered notification.")

Cleanup scope: clean up notifications for all states except `.registration`.

## From PR Reviews

- "We shouldn't fallback to raw bytes here - throw an error. Always expect decodable call."
- "Use `utf8View` to prevent optional" — prefer safe conversion APIs
