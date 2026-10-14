import Foundation

public enum ConnectionNodeSwitchCode {
    public static let infura = -32_005
    public static let alchemy = 429
    public static let blustCapacity = -32_098
    public static let blustRateLimit = -32_097

    public static var allCodes: Set<Int> {
        [infura, alchemy, blustCapacity, blustRateLimit]
    }
}
