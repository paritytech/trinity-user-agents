#if SENTRY_ENABLED
    import Foundation
    import Sentry

    struct SentryHostEventRecorder: HostEventRecording {
        func record(_ event: @autoclosure () -> HostEvent) {
            let event = event()

            switch event.route {
            case let .log(body, attributes):
                SentrySDK.logger.info(body, attributes: attributes.mapValues(\.sentryValue))
            case .capture:
                capture(event)
            }
        }

        private func capture(_ event: HostEvent) {
            let sentryEvent = Event(level: event.level.sentryLevel)
            sentryEvent.message = SentryMessage(formatted: event.name)
            sentryEvent.logger = HostEvent.logTag
            sentryEvent.fingerprint = event.fingerprint
            sentryEvent.tags = event.tags
            sentryEvent.extra = event.extras.mapValues(\.sentryValue)

            // The scope deep merges its own contexts into this one, so naming a
            // key here adds a section rather than replacing device, app and os.
            if !event.counters.isEmpty {
                sentryEvent.context = ["counters": event.counters]
            }

            SentrySDK.capture(event: sentryEvent)
        }
    }

    private extension HostLogLevel {
        var sentryLevel: SentryLevel {
            switch self {
            case .info: .info
            case .warning: .warning
            case .error: .error
            }
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
