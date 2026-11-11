#if SENTRY_ENABLED
    import Foundation
    import Sentry

    /// Sends host events as Sentry structured logs, not error events: they are measurements, and as events
    /// Sentry filed each name as an issue of its own.
    ///
    /// Every tag and extra becomes a typed log attribute, so `host.session_id`, `open.source`, `tap_ms` and the
    /// rest stay searchable. The log's body and `event.name` are the event's name; `tag` is `host.telemetry`,
    /// the same attribute the host's other log lines carry.
    struct SentryHostEventRecorder: HostEventRecording {
        func record(_ event: @autoclosure () -> HostEvent) {
            let event = event()
            SentrySDK.logger.info(event.name, attributes: event.sentryLogAttributes)
        }
    }

    extension HostEvent {
        var sentryLogAttributes: [String: Any] {
            var attributes: [String: Any] = tags
            for (key, value) in extras {
                attributes[key] = value.sentryValue
            }
            attributes["event.name"] = name
            attributes["tag"] = "host.telemetry"
            return attributes
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
