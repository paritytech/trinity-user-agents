#if SENTRY_ENABLED
    import Foundation
    import Sentry

    final class SentryIssueMonitoringService: IssueMonitoringServiceProtocol {
        private let dsn: String
        private let logFilter = HostLogFilter()

        init(dsn: String) {
            self.dsn = dsn
        }

        func setup() {
            SentrySDK.start { [dsn, logFilter] options in
                options.dsn = dsn

                // Adds IP for users.
                // For more information, visit: https://docs.sentry.io/platforms/apple/data-management/data-collected/
                options.sendDefaultPii = false

                // Breadcrumbs can carry log lines and URLs; critical issues are reported without them.
                options.maxBreadcrumbs = 0

                // Set tracesSampleRate to 1.0 to capture 100% of transactions for performance monitoring.
                // We recommend adjusting this value in production.
                options.tracesSampleRate = 0

                // The last gate a log passes, whichever route reached the logger, so a line this
                // host did not write is turned away here and the rest are limited per marker.
                options.beforeSendLog = { log in
                    let tag = log.attributes[HostLogFilter.tagAttribute]?.value as? String
                    let eventName = log.attributes[HostLogFilter.eventNameAttribute]?.value as? String
                    switch logFilter.verdict(tag: tag, body: log.body, eventName: eventName) {
                    case .drop:
                        return nil
                    case let .send(dropped):
                        if dropped > 0 {
                            log.attributes[HostLogFilter.droppedAttribute] = SentryAttribute(integer: dropped)
                        }
                        return log
                    }
                }
            }
        }
    }
#endif
