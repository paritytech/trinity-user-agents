#if SENTRY_ENABLED
    import Foundation
    import Sentry

    struct SentryIssueReporter: IssueReporting {
        func report(_ issue: CriticalIssue) {
            SentrySDK.capture(message: "\(issue.flow).\(issue.kind)") { scope in
                scope.setFingerprint([issue.flow, issue.kind])
                scope.setTag(value: issue.flow, key: "flow")
                scope.setTag(value: issue.kind, key: "kind")
                issue.errorType.map { scope.setTag(value: $0, key: "errorType") }
                issue.errorCode.map { scope.setTag(value: $0, key: "errorCode") }
                scope.setContext(value: issue.counters, key: "counters")
            }
        }

        func report(_ event: GameReminderEvent) {
            let fields = event.fields(origin: .current)
            let sentryEvent = Event(level: .info)
            sentryEvent.message = SentryMessage(formatted: event.name)
            sentryEvent.logger = GameReminderEvent.logger
            SentrySDK.capture(event: sentryEvent) { scope in
                scope.setFingerprint([event.name])
                fields.tags.forEach { scope.setTag(value: $0.value, key: $0.key) }
                fields.extras.forEach { scope.setExtra(value: $0.value, key: $0.key) }
            }
        }
    }
#endif
