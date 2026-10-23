import Products
import TrUAPIHost

extension PermissionAuthorizationRequest {
    init(_ permission: ProductPermission) {
        self =
            switch permission {
            case let .deviceCapability(capability):
                .device(capability.hostRequest)
            case let .networkAccess(domain):
                .remote(RemotePermissionRequest(permission: .remote(domains: [domain])))
            case let .accountAccess(targetProductId):
                .accountAccess(targetProductId: targetProductId)
            case .balanceAccess:
                .remote(RemotePermissionRequest(permission: .balanceAccess))
            case .webRtcAccess:
                .remote(RemotePermissionRequest(permission: .webRtc))
            case .chainSubmitAccess:
                .remote(RemotePermissionRequest(permission: .chainSubmit))
            case .preimageSubmitAccess:
                .remote(RemotePermissionRequest(permission: .preimageSubmit))
            case .statementSubmitAccess:
                .remote(RemotePermissionRequest(permission: .statementSubmit))
            case .userIdentityAccess:
                .identityDisclosure
            }
    }
}

extension DeviceCapabilityType {
    var hostRequest: HostDevicePermissionRequest {
        switch self {
        case .notifications: .notifications
        case .camera: .camera
        case .microphone: .microphone
        case .bluetooth: .bluetooth
        case .nfc: .nfc
        case .location: .location
        case .clipboard: .clipboard
        case .openUrl: .openUrl
        case .biometrics: .biometrics
        case .motion: .motion
        }
    }
}
