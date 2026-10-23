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

                // Breadcrumbs can carry log lines and URLs; critical issues are reported without them.
                options.maxBreadcrumbs = 0

                // Set tracesSampleRate to 1.0 to capture 100% of transactions for performance monitoring.
                // We recommend adjusting this value in production.
                options.tracesSampleRate = 1
                // Automatic HTTP spans include full URLs; enable only when a URL scrubber is available.
                options.enableNetworkTracking = false
            }
        }
    }
#endif
