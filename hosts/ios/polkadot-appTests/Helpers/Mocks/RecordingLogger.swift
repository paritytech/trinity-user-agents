import Foundation
import SDKLogger

/// Records each line with its level, because the level decides whether a line leaves the device.
final class RecordingLogger: SDKLoggerProtocol, @unchecked Sendable {
    enum Level: Equatable {
        case verbose
        case debug
        case info
        case warning
        case error
    }

    struct Line: Equatable {
        let level: Level
        let message: String

        static func debug(_ message: String) -> Line {
            Line(level: .debug, message: message)
        }

        static func info(_ message: String) -> Line {
            Line(level: .info, message: message)
        }
    }

    private let lock = NSLock()
    private var recorded: [Line] = []

    var lines: [Line] {
        lock.withLock { recorded }
    }

    func verbose(message: () -> String, file _: String, function _: String, line _: Int) {
        record(.verbose, message())
    }

    func debug(message: () -> String, file _: String, function _: String, line _: Int) {
        record(.debug, message())
    }

    func info(message: () -> String, file _: String, function _: String, line _: Int) {
        record(.info, message())
    }

    func warning(message: () -> String, file _: String, function _: String, line _: Int) {
        record(.warning, message())
    }

    func error(message: () -> String, file _: String, function _: String, line _: Int) {
        record(.error, message())
    }

    private func record(_ level: Level, _ message: String) {
        lock.withLock { recorded.append(Line(level: level, message: message)) }
    }
}
