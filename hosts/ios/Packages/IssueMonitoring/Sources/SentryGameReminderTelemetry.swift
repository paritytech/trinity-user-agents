#if SENTRY_ENABLED
    import Foundation
    import Sentry

    struct SentryGameReminderTelemetry: GameReminderTelemetry {
        func record(_ event: GameReminderEvent) {
            let fields = event.sentryFields(origin: .current)
            SentrySDK.capture(message: event.name) { scope in
                scope.setFingerprint([event.name])
                fields.tags.forEach { scope.setTag(value: $0.value, key: $0.key) }
                fields.extras.forEach { scope.setExtra(value: $0.value, key: $0.key) }
            }
        }
    }
#endif
