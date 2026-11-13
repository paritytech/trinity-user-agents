#if SENTRY_ENABLED
    import Foundation
    import Sentry

    /// Sends info-level host events, which are measurements such as `host.webview.created`, as Sentry structured
    /// logs rather than error events, because as events Sentry filed each name as an issue of its own. Warnings and
    /// errors, such as the issues `HostEventIssueReporter` sends, stay Sentry events with their level, fingerprint
    /// and counters.
    ///
    /// Every tag, extra and counter becomes a typed log attribute (`HostEvent.logAttributes`), so
    /// `host.session_id`, `open.source`, `tap_ms` and the rest stay searchable. The log's body is the event's name.
    struct SentryHostEventRecorder: HostEventRecording {
        func record(_ event: @autoclosure () -> HostEvent) {
            let event = event()

            guard event.level != .info else {
                SentrySDK.logger.info(event.name, attributes: event.logAttributes.mapValues(\.sentryValue))
                return
            }

            let sentryEvent = Event(level: event.level.sentryLevel)
            sentryEvent.message = SentryMessage(formatted: event.name)
            sentryEvent.logger = "host.telemetry"
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
