import Foundation

public enum MessageExchangeMode {
    /// V1 — identity-level sessions only, no device subscriptions or broadcasts.
    case identity

    /// V2 — multidevice handshake with device-level subscriptions and broadcasts.
    case multidevice
}
