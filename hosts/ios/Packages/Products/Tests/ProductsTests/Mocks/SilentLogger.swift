import Foundation
import SDKLogger

/// Manifest rejections are logged, and the tests assert on the rejection, not the message.
final class SilentLogger: SDKLoggerProtocol {
    func verbose(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func debug(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func info(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func warning(message _: () -> String, file _: String, function _: String, line _: Int) {}
    func error(message _: () -> String, file _: String, function _: String, line _: Int) {}
}
