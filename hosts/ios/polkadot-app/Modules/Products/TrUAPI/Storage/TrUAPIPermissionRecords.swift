import Foundation
import Products
import TrUAPIHost

extension Products.ProductPermission {
    func coreRequest() throws -> PermissionAuthorizationRequest {
        switch self {
        case let .deviceCapability(capability):
            return .device(capability.coreRequest)
        case let .accountAccess(target):
            return .accountAccess(targetProductId: target)
        case .userIdentityAccess:
            return .identityDisclosure
        case .balanceAccess:
            throw HostRejection.Rejected(reason: "Balance disclosure is not a persisted TrUAPI permission")
        case let .networkAccess(domain):
            return .remote(.init(permission: .remote(domains: [domain])))
        case .webRtcAccess:
            return .remote(.init(permission: .webRtc))
        case .chainSubmitAccess:
            return .remote(.init(permission: .chainSubmit))
        case .preimageSubmitAccess:
            return .remote(.init(permission: .preimageSubmit))
        case .statementSubmitAccess:
            return .remote(.init(permission: .statementSubmit))
        }
    }
}

private extension DeviceCapabilityType {
    var coreRequest: HostDevicePermissionRequest {
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
        }
    }
}

extension PermissionRecord {
    var productGrants: [ProductPermissionGrant] {
        let permissions: [Products.ProductPermission]
        switch request {
        case let .device(capability): permissions = [.deviceCapability(capability.deviceCapabilityType)]
        case let .accountAccess(target): permissions = [.accountAccess(targetProductId: target)]
        case .identityDisclosure: permissions = [.userIdentityAccess]
        case let .remote(remote): permissions = remote.permission.toDomainRequest().toDomainPermissions()
        }
        return permissions.map {
            ProductPermissionGrant(
                productId: productId, permission: $0, granted: status == .authorized, grantedAt: nil
            )
        }
    }
}
