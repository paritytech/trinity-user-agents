#if SENTRY_ENABLED
    import Foundation
    import Sentry

    final class SentryIssueMonitoringService: IssueMonitoringServiceProtocol {
        private let dsn: String

        init(dsn: String) {
            self.dsn = dsn
        }

        func setup() {
            SentrySDK.start { [dsn] options in
                options.dsn = dsn

                // Adds IP for users.
                // For more information, visit: https://docs.sentry.io/platforms/apple/data-management/data-collected/
                options.sendDefaultPii = false

                // Nightly telemetry is for internal PRE investigations. Keep the events leading
                // up to a failure so a report explains what the app was doing beforehand.
                options.maxBreadcrumbs = 100

                // Set tracesSampleRate to 1.0 to capture 100% of transactions for performance monitoring.
                options.tracesSampleRate = 1
            }
        }
    }
#endif
