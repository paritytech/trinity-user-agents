#if SENTRY_ENABLED
    import Foundation
    import Sentry

    struct SentryHostLogSink: HostLogSink {
        func log(level: HostLogLevel, tag: String, message: String, attributes: [String: HostLogAttribute]) {
            var fields: [String: Any] = attributes.mapValues(\.sentryValue)
            fields["tag"] = tag

            switch level {
            case .info:
                SentrySDK.logger.info(message, attributes: fields)
            case .warning:
                SentrySDK.logger.warn(message, attributes: fields)
            case .error:
                SentrySDK.logger.error(message, attributes: fields)
            }
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
