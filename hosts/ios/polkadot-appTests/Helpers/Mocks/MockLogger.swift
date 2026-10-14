import Foundation
import SDKLogger
import StructuredConcurrency

// MARK: - Mock Logger

/// Records error messages so a test can wait for the code under test to reach its failure path.
final class MockLogger: SDKLoggerProtocol, @unchecked Sendable {
    private let lock = NSLock()
    private var recordedErrors: [String] = []
    private let errorStream: AsyncStream<String>
    private let errorContinuation: AsyncStream<String>.Continuation

    init() {
        (errorStream, errorContinuation) = AsyncStream.makeStream(bufferingPolicy: .unbounded)
    }

    var errors: [String] {
        lock.lock()
        defer { lock.unlock() }
        return recordedErrors
    }

    struct ErrorNotLogged: Error {
        let expected: String
        let recorded: [String]
    }

    /// Returns once an error containing `fragment` has been logged, including one logged before the
    /// call; fails after `timeout` so a code path that never logs cannot hang the test.
    func waitForError(containing fragment: String, timeout: Duration = .seconds(5)) async throws {
        do {
            try await withTimeout(timeout) { [errorStream] in
                for await message in errorStream where message.contains(fragment) {
                    return
                }
            }
        } catch is TimeoutError {
            throw ErrorNotLogged(expected: fragment, recorded: errors)
        }
    }

    func verbose(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func debug(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func info(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func warning(message _: () -> String, file _: String, function _: String, line _: Int) {}

    func error(message: () -> String, file _: String, function _: String, line _: Int) {
        let text = message()
        lock.lock()
        recordedErrors.append(text)
        lock.unlock()
        errorContinuation.yield(text)
    }
}
