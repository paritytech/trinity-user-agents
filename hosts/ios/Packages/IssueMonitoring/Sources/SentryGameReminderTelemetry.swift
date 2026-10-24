#if SENTRY_ENABLED
    import Foundation
    import Sentry

    struct SentryGameReminderTelemetry: GameReminderTelemetry {
        func record(_ event: GameReminderEvent) {
            let fields = event.sentryFields(origin: .current)
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
