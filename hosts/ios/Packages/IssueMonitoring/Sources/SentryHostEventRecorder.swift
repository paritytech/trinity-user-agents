#if SENTRY_ENABLED
    import Foundation
    import Sentry

    struct SentryHostEventRecorder: HostEventRecording {
        func record(_ event: @autoclosure () -> HostEvent) {
            let event = event()
            let sentryEvent = Event(level: .info)
            sentryEvent.message = SentryMessage(formatted: event.name)
            sentryEvent.logger = "host.telemetry"
            sentryEvent.fingerprint = [event.name]
            sentryEvent.tags = event.tags
            sentryEvent.extra = event.extras.mapValues(\.sentryValue)

            SentrySDK.capture(event: sentryEvent)
        }
    }

    private extension HostEventValue {
        var sentryValue: Any {
            switch self {
            case let .string(value): value
            case let .integer(value): value
            case let .double(value): value
            case let .boolean(value): value
            }
        }
    }
#endif
