import Foundation
import SDKLogger

// MARK: - Mock Logger

final class MockLogger: SDKLoggerProtocol, @unchecked Sendable {
    func verbose(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func debug(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func info(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func warning(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func error(message _: () -> String, file _: String, function _: String, line _: Int) {}
}
