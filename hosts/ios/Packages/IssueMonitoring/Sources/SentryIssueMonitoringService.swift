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
                options.tracesSampleRate = 0
            }
        }

        func recordProcessStarted(_ context: HostProcessStartedContext) {
            SentrySDK.capture(message: context.eventName) { scope in
                scope.setFingerprint([context.eventName])
                scope.setTag(value: context.appLaunchID.uuidString.lowercased(), key: "app.launch_id")
                context.appVersion.map { scope.setTag(value: $0, key: "host.native_app_version") }
                scope.setTag(value: context.appVariant, key: "host.app_variant")
                scope.setTag(value: context.platform, key: "host.platform")
                context.osVersion.map { scope.setTag(value: $0, key: "host.os_version") }
                context.truapiHostVersion.map { scope.setTag(value: $0, key: "host.truapi_host_version") }
                context.appBuild.map { scope.setExtra(value: $0, key: "host.app_build") }
                scope.setExtra(value: context.monotonicMilliseconds, key: "event.monotonic_ms")
            }
        }
    }
#endif
