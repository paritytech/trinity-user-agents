import os
import SDKLogger

final class RecordingLogger: SDKLoggerProtocol, @unchecked Sendable {
    private let lines = OSAllocatedUnfairLock<[String]>(initialState: [])

    var messages: [String] {
        lines.withLock { $0 }
    }

    func verbose(message: () -> String, file _: String, function _: String, line _: Int) {
        record(message())
    }

    func debug(message: () -> String, file _: String, function _: String, line _: Int) {
        record(message())
    }

    func info(message: () -> String, file _: String, function _: String, line _: Int) {
        record(message())
    }

    func warning(message: () -> String, file _: String, function _: String, line _: Int) {
        record(message())
    }

    func error(message: () -> String, file _: String, function _: String, line _: Int) {
        record(message())
    }

    private func record(_ message: String) {
        lines.withLock { $0.append(message) }
    }
}
