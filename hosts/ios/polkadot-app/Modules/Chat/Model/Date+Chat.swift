import Foundation

extension Date {
    func toChatTimestamp() -> UInt64 {
        UInt64(timeIntervalSince1970 * 1_000)
    }

    static func fromChatTimestamp(_ timestamp: UInt64) -> Date {
        // TimeInterval can hold integers only up to 2^53 but for near thousand years we are safe
        Date(timeIntervalSince1970: TimeInterval(timestamp) / 1_000)
    }
}
