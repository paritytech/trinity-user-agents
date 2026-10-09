import Foundation
import PolkadotUI
import Products

protocol AppPermissionsViewModelMaking {
    func createItems(from grants: [ProductPermissionGrant]) -> [AppPermissionsViewLayout.Item]
}

final class AppPermissionsViewModelFactory {
    init() {}
}

extension AppPermissionsViewModelFactory: AppPermissionsViewModelMaking {
    func createItems(from grants: [ProductPermissionGrant]) -> [AppPermissionsViewLayout.Item] {
        grants.map { grant in
            AppPermissionsViewLayout.Item(
                id: grant.identifier,
                title: grant.permission.settingsTitle,
                description: grant.permission.permissionDescription,
                isOn: true
            )
        }
    }
}

/// Shared descriptions keep consent prompts and the revocation screen in agreement.
extension ProductPermission {
    var permissionDescription: String {
        switch self {
        case let .deviceCapability(capability): capabilityDescription(capability)
        case let .networkAccess(domain):
            String(localized: .Products.permissionBodyNetworkAccess(domain: domain))
        case let .networkAccessBundle(domains):
            String(localized: .Products.permissionBodyNetworkAccess(domain: domains.joined(separator: ", ")))
        case let .accountAccess(targetProductId):
            String(localized: .Products.permissionBodyAccountAccess(targetProductId: targetProductId))
        case .balanceAccess: String(localized: .Products.permissionBodyBalanceAccess)
        case .webRtcAccess: String(localized: .Products.permissionBodyWebRtc)
        case .chainSubmitAccess: String(localized: .Products.permissionBodyChainSubmit)
        case .preimageSubmitAccess: String(localized: .Products.permissionBodyPreimageSubmit)
        case .statementSubmitAccess: String(localized: .Products.permissionBodyStatementSubmit)
        case let .jamPeersAccess(genesis):
            String(localized: .Products.permissionLabelJamPeers(genesis: genesis))
        case .userIdentityAccess: String(localized: .Products.permissionBodyUserIdentityAccess)
        }
    }

    var permissionIconSystemName: String {
        switch self {
        case let .deviceCapability(capability): capabilityIcon(capability)
        case .networkAccess, .networkAccessBundle: "globe"
        case .accountAccess: "person.crop.circle"
        case .balanceAccess: "dollarsign.circle.fill"
        case .webRtcAccess: "video.fill"
        case .chainSubmitAccess: "link"
        case .preimageSubmitAccess: "doc.text"
        case .statementSubmitAccess: "text.bubble"
        case .jamPeersAccess: "point.3.connected.trianglepath.dotted"
        case .userIdentityAccess: "person.text.rectangle"
        }
    }

    var settingsTitle: String {
        switch self {
        case let .deviceCapability(capability): capabilityTitle(capability)
        case .networkAccess, .networkAccessBundle: String(localized: .Products.appPermissionNetworkTitle)
        case .accountAccess: String(localized: .Products.appPermissionAccountTitle)
        case .balanceAccess: String(localized: .Products.appPermissionBalanceTitle)
        case .webRtcAccess: String(localized: .Products.appPermissionWebRtcTitle)
        case .chainSubmitAccess: String(localized: .Products.appPermissionChainSubmitTitle)
        case .preimageSubmitAccess: String(localized: .Products.appPermissionPreimageSubmitTitle)
        case .statementSubmitAccess: String(localized: .Products.appPermissionStatementSubmitTitle)
        case .jamPeersAccess: String(localized: .Products.appPermissionJamPeersTitle)
        case .userIdentityAccess: String(localized: .Products.appPermissionUserIdentityTitle)
        }
    }
}

private extension ProductPermission {
    func capabilityTitle(_ capability: DeviceCapabilityType) -> String {
        switch capability {
        case .notifications: String(localized: .Products.appPermissionCapabilityNotifications)
        case .camera: String(localized: .Products.appPermissionCapabilityCamera)
        case .microphone: String(localized: .Products.appPermissionCapabilityMicrophone)
        case .bluetooth: String(localized: .Products.appPermissionCapabilityBluetooth)
        case .nfc: String(localized: .Products.appPermissionCapabilityNfc)
        case .location: String(localized: .Products.appPermissionCapabilityLocation)
        case .clipboard: String(localized: .Products.appPermissionCapabilityClipboard)
        case .openUrl: String(localized: .Products.appPermissionCapabilityOpenUrl)
        case .biometrics: String(localized: .Products.appPermissionCapabilityBiometrics)
        }
    }

    func capabilityDescription(_ capability: DeviceCapabilityType) -> String {
        switch capability {
        case .notifications: String(localized: .Products.permissionCapabilityDescriptionNotifications)
        case .camera: String(localized: .Products.permissionCapabilityDescriptionCamera)
        case .microphone: String(localized: .Products.permissionCapabilityDescriptionMicrophone)
        case .bluetooth: String(localized: .Products.permissionCapabilityDescriptionBluetooth)
        case .nfc: String(localized: .Products.permissionCapabilityDescriptionNfc)
        case .location: String(localized: .Products.permissionCapabilityDescriptionLocation)
        case .clipboard: String(localized: .Products.permissionCapabilityDescriptionClipboard)
        case .openUrl: String(localized: .Products.permissionCapabilityDescriptionOpenUrl)
        case .biometrics: String(localized: .Products.permissionCapabilityDescriptionBiometrics)
        }
    }

    func capabilityIcon(_ capability: DeviceCapabilityType) -> String {
        switch capability {
        case .notifications: "bell.fill"
        case .camera: "camera.fill"
        case .microphone: "mic.fill"
        case .bluetooth: "antenna.radiowaves.left.and.right"
        case .nfc: "wave.3.right"
        case .location: "location.fill"
        case .clipboard: "doc.on.clipboard.fill"
        case .openUrl: "safari.fill"
        case .biometrics: "faceid"
        }
    }
}
