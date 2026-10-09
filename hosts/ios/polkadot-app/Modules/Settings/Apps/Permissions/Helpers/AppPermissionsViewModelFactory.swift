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
            let display = displayInfo(for: grant.permission)
            return AppPermissionsViewLayout.Item(
                id: grant.identifier,
                title: display.title,
                description: display.description,
                isOn: true
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
        case let .networkAccessBundle(domains):
            displayInfo(for: .networkAccess(domain: domains.joined(separator: ", ")))
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
        case .chatAuthority:
            (
                String(localized: .Products.appPermissionChatAuthorityTitle),
                String(localized: .Products.permissionBodyChatAuthority)
            )
        case .profileDisclosure:
            (
                String(localized: .Products.appPermissionProfileDisclosureTitle),
                String(localized: .Products.permissionBodyProfileDisclosure)
            )
        case let .statementStoreAllowance(derivationIndex):
            (
                String(localized: .Products.appPermissionStatementStoreAllowanceTitle),
                ProductPermission.statementStoreAllowanceDescription(derivationIndex: derivationIndex)
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

extension ProductPermission {
    static func statementStoreAllowanceDescription(derivationIndex: ProductAccountSelector?) -> String {
        switch derivationIndex {
        case nil:
            String(localized: .Products.permissionBodyStatementStoreAllowanceLegacy)
        case let .index(index):
            String(localized: .Products.permissionBodyStatementStoreAllowanceIndex(index: String(index)))
        case let .raw(bytes):
            String(localized: .Products.permissionBodyStatementStoreAllowanceRaw(
                selector: "0x" + bytes.map { String(format: "%02x", $0) }.joined()
            ))
        }
    }
}
