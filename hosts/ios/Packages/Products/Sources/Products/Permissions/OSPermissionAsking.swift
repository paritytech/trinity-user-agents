import Foundation

public enum OSPermissionStatus: Equatable {
    case allowed
    case denied
    case notDetermined

    public var isAllowed: Bool {
        self == .allowed
    }

    public var isDenied: Bool {
        self == .denied
    }

    public var isNotDetermined: Bool {
        self == .notDetermined
    }
}

/// Asks the operating system for a runtime permission that maps to a
/// ``DeviceCapabilityType``.
public protocol OSPermissionAsking: Sendable {
    func checkPermission(for capability: DeviceCapabilityType) async -> OSPermissionStatus
    func requestPermission(for capability: DeviceCapabilityType) async -> Bool
}
