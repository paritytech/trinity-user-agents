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
            case let .nftPurseRead(ownerProductId):
                .remote(RemotePermissionRequest(permission: .nftPurseRead(owner: ownerProductId
                        .map { .product($0) } ?? .root)))
            }
    }

    /// The settings permissions a stored core decision stands for: none for a
    /// decision the settings never list, one per domain for a domain bundle.
    var productPermissions: [ProductPermission] {
        switch self {
        case let .device(request):
            [.deviceCapability(request.deviceCapabilityType)]
        case let .remote(request):
            request.permission.toDomainPermissions()
        case .identityDisclosure:
            [.userIdentityAccess]
        case let .accountAccess(targetProductId):
            [.accountAccess(targetProductId: targetProductId)]
        case .crossProductSignature:
            []
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
