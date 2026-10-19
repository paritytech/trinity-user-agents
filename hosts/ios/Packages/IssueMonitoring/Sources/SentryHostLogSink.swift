#if SENTRY_ENABLED
    import Foundation
    import Sentry

    struct SentryHostLogSink: HostLogSink {
        func log(level: HostLogLevel, tag: String, message: String) {
            let attributes: [String: Any] = ["tag": tag]

            switch level {
            case .info:
                SentrySDK.logger.info(message, attributes: attributes)
            case .warning:
                SentrySDK.logger.warn(message, attributes: attributes)
            case .error:
                SentrySDK.logger.error(message, attributes: attributes)
            }
        }
    }
#endif
