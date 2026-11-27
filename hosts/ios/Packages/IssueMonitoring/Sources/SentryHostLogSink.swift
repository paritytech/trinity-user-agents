#if SENTRY_ENABLED
    import Foundation
    import Sentry

    /// Holds no state: `SentrySDK.logger` is read on every call, since `SentryLogger` is not `Sendable`.
    struct SentryHostLogSink: HostLogSink {
        func log(level: HostLogLevel, tag: String, message: String, attributes: [String: HostLogAttribute]) {
            var fields: [String: Any] = attributes.mapValues(\.sentryValue)
            fields[HostLogFilter.tagAttribute] = tag

            let logger = SentrySDK.logger

            let send: (String, [String: Any]) -> Void =
                switch level {
                case .info: logger.info
                case .warning: logger.warn
                case .error: logger.error
                }

            send(message, fields)
        }
    }

    private extension HostLogAttribute {
        var sentryValue: Any {
            switch self {
            case let .text(value):
                value
            case let .count(value):
                value
            case let .flag(value):
                value
            }
        }
    }
#endif
