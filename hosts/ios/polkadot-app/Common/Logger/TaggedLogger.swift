import Foundation

final class TaggedLogger: Sendable {
    private let tag: String
    private let baseLogger: LoggerProtocol

    init(tag: String, logger baseLogger: LoggerProtocol) {
        self.tag = tag
        self.baseLogger = baseLogger
    }
}

extension TaggedLogger: LoggerProtocol {
    func verbose(message: () -> String, file: String, function: String, line: Int) {
        baseLogger.verbose(message: { tagged(message()) }, file: file, function: function, line: line)
    }

    func debug(message: () -> String, file: String, function: String, line: Int) {
        baseLogger.debug(message: { tagged(message()) }, file: file, function: function, line: line)
    }

    func info(message: () -> String, file: String, function: String, line: Int) {
        baseLogger.info(message: { tagged(message()) }, file: file, function: function, line: line)
    }

    func warning(message: () -> String, file: String, function: String, line: Int) {
        baseLogger.warning(message: { tagged(message()) }, file: file, function: function, line: line)
    }

    func error(message: () -> String, file: String, function: String, line: Int) {
        baseLogger.error(message: { tagged(message()) }, file: file, function: function, line: line)
    }
}

private extension TaggedLogger {
    func tagged(_ message: String) -> String {
        "[\(tag)] \(message)"
    }
}
