import Foundation
import WebKit

public enum JSDeviceCapability: Sendable {
    case camera
    case microphone
    case motion

    public var deviceCapabilityType: DeviceCapabilityType {
        switch self {
        case .camera:
            .camera
        case .microphone:
            .microphone
        case .motion:
            .motion
        }
    }
}

public enum JSDeviceCapabilityDecision: Sendable {
    case allowed
    case denied
}

public typealias JSDeviceCapabilityHandler = @Sendable (JSDeviceCapability) async throws -> JSDeviceCapabilityDecision

public extension JSDeviceCapabilityDecision {
    var toWKPermission: WKPermissionDecision {
        switch self {
        case .allowed:
            .grant
        case .denied:
            .deny
        }
    }
}
