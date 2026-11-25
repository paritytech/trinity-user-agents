import IssueMonitoring
import SwiftyBeaver

/// The fields of one core event, carried to log destinations as SwiftyBeaver context.
struct CoreEventContext: Sendable {
    let marker: String
    let attributes: [String: HostLogAttribute]
}

/// A logger that can carry a core event's attributes to its destinations as fields.
protocol CoreEventLoggerProtocol: LoggerProtocol {
    /// Logs at `level`, or at debug when it is nil.
    func log(level: HostLogLevel?, message: String, context: CoreEventContext)
}

extension Logger: CoreEventLoggerProtocol {
    func log(level: HostLogLevel?, message: String, context: CoreEventContext) {
        log.custom(level: level?.swiftyBeaverLevel ?? .debug, message: message, context: context)
    }
}

private extension HostLogLevel {
    var swiftyBeaverLevel: SwiftyBeaver.Level {
        switch self {
        case .info:
            .info
        case .warning:
            .warning
        case .error:
            .error
        }
    }
}
