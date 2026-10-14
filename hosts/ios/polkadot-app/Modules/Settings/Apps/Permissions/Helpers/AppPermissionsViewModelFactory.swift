import Foundation
import PolkadotUI
import Products

protocol AppPermissionsViewModelMaking {
    func createItems(
        from grants: [ProductPermissionGrant],
        pendingDeletionIds: Set<String>
    ) -> [AppPermissionsViewLayout.Item]
}

final class AppPermissionsViewModelFactory {
    init() {}
}

extension AppPermissionsViewModelFactory: AppPermissionsViewModelMaking {
    func createItems(
        from grants: [ProductPermissionGrant],
        pendingDeletionIds: Set<String>
    ) -> [AppPermissionsViewLayout.Item] {
        grants.map { grant in
            let display = displayInfo(for: grant.permission)
            let isOn = !pendingDeletionIds.contains(grant.identifier)
            return AppPermissionsViewLayout.Item(
                id: grant.identifier,
                title: display.title,
                description: display.description,
                isOn: isOn
            )
        }
    }
}

private extension AppPermissionsViewModelFactory {
    typealias DisplayInfo = (title: String, description: String)

    func displayInfo(for permission: ProductPermission) -> DisplayInfo {
        switch permission {
        case let .deviceCapability(capability):
            (capabilityTitle(capability), capabilityDescription(capability))
        case let .networkAccess(domain):
            (
                String(localized: .Products.appPermissionNetworkTitle),
                String(localized: .Products.permissionBodyNetworkAccess(domain: domain))
            )
        case let .accountAccess(targetProductId):
            (
                String(localized: .Products.appPermissionAccountTitle),
                String(
                    localized: .Products.permissionBodyAccountAccess(
                        targetProductId: targetProductId
                    )
                )
            )
        case .balanceAccess:
            (
                String(localized: .Products.appPermissionBalanceTitle),
                String(localized: .Products.permissionBodyBalanceAccess)
            )
        case .webRtcAccess:
            (
                String(localized: .Products.appPermissionWebRtcTitle),
                String(localized: .Products.permissionBodyWebRtc)
            )
        case .chainSubmitAccess:
            (
                String(localized: .Products.appPermissionChainSubmitTitle),
                String(localized: .Products.permissionBodyChainSubmit)
            )
        case .preimageSubmitAccess:
            (
                String(localized: .Products.appPermissionPreimageSubmitTitle),
                String(localized: .Products.permissionBodyPreimageSubmit)
            )
        case .statementSubmitAccess:
            (
                String(localized: .Products.appPermissionStatementSubmitTitle),
                String(localized: .Products.permissionBodyStatementSubmit)
            )
        case .userIdentityAccess:
            (
                String(localized: .Products.appPermissionUserIdentityTitle),
                String(localized: .Products.permissionBodyUserIdentityAccess)
            )
        }
    }

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
}
