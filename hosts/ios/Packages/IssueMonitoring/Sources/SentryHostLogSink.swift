#if SENTRY_ENABLED
    import Foundation
    @preconcurrency import Sentry

    struct SentryHostLogSink: HostLogSink {
        private let logger: SentryLogger

        init(logger: SentryLogger) {
            self.logger = logger
        }

        func log(level: HostLogLevel, tag: String, message: String, attributes: [String: HostLogAttribute]) {
            var fields: [String: Any] = attributes.mapValues(\.sentryValue)
            fields["tag"] = tag

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
