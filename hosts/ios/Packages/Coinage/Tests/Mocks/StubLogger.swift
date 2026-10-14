import Foundation
import SDKLogger

/// A logger that swallows every message — for tests that don't assert on logging.
final class StubLogger: SDKLoggerProtocol {
    func verbose(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func debug(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func info(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func warning(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func error(message _: () -> String, file _: String, function _: String, line _: Int) {}
}
