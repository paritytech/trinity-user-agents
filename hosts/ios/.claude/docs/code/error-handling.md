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
