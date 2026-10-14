import Foundation

// Timeout settings to apply for JSONRPC requests

public enum JSONRPCTimeout {
    // it is better to retry with another node then to wait long
    public static let withNodeSwitch: Int = 15

    // there is a single node so wait as much as we can
    public static let singleNode: Int = 60

    public static let hour: Int = 60 * 60
}
