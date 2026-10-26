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
    }
#endif
