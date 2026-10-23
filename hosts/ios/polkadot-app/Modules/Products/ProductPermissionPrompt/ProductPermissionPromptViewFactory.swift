import Foundation_iOS
import Products
import UIKit
import PolkadotUI
import UIKitExt

@MainActor
enum ProductPermissionPromptViewFactory {
    static func createView(context: ProductPermissionContext) -> ControllerBackedProtocol {
        let viewModel = makeViewModel(for: context)
        let styler = ProductPromptStyler()

        let view = TitleDetailsSheetViewFactory.createView(
            from: viewModel,
            styler: styler,
            allowsSwipeDown: false
        )

        BottomSheetViewFacade.setupBottomSheet(from: view.controller)

        return view
    }
}

// MARK: - Prompt Content

private struct PromptContent {
    let title: String
    let body: String
    let icon: UIImage?
}

// MARK: - ViewModel Building

private extension ProductPermissionPromptViewFactory {
    static func makeViewModel(
        for context: ProductPermissionContext
    ) -> TitleDetailsSheetViewModel {
        let content: PromptContent =
            if context.permissions.count == 1, let permission = context.permissions.first {
                makeSingleContent(
                    productId: context.productId,
                    permission: permission
                )
            } else {
                makeBatchedContent(
                    productId: context.productId,
                    permissions: context.permissions
                )
            }

        let manageHint = String(localized: .Products.permissionBodyManageInSettingsHint)
        let bodyWithHint = "\(content.body) \(manageHint)"

        return TitleDetailsSheetViewModel(
            graphics: content.icon,
            title: LocalizableResource { _ in content.title },
            message: LocalizableResource { _ in .normal(bodyWithHint) },
            mainAction: makeAction(
                title: String(localized: .Products.permissionActionAllowAlways)
            ) { context.deliver(.allowAlways) },
            secondaryAction: makeAction(
                title: String(localized: .Products.permissionActionAllowOnce)
            ) { context.deliver(.allowOnce) },
            tertiaryAction: makeAction(
                title: String(localized: .Products.permissionActionDeny)
            ) { context.deliver(.deny) }
        )
    }

    static func makeSingleContent(
        productId: String,
        permission: ProductPermission
    ) -> PromptContent {
        let title: String
        switch permission {
        case let .deviceCapability(capability):
            title = String(
                localized: .Products.permissionTitleDeviceCapability(
                    productId: productId,
                    capability: capabilityDisplayName(capability)
                )
            )
        case .networkAccess, .networkAccessBundle:
            title = String(localized: .Products.permissionTitleNetworkAccess(productId: productId))
        case .accountAccess:
            title = String(localized: .Products.permissionTitleAccountAccess(productId: productId))
        case .balanceAccess,
             .webRtcAccess,
             .chainSubmitAccess,
             .preimageSubmitAccess,
             .statementSubmitAccess,
             .userIdentityAccess:
            title = String(localized: .Products.permissionTitleRemote(productId: productId))
        case .chatAuthority:
            title = String(localized: .Products.permissionTitleChatAuthority(productId: productId))
        case .profileDisclosure:
            title = String(localized: .Products.permissionTitleProfileDisclosure(productId: productId))
        case .statementStoreAllowance:
            title = String(localized: .Products.permissionTitleStatementStoreAllowance(productId: productId))
        case let .nftPurseRead(ownerProductId):
            return PromptContent(
                title: String(localized: .Products.permissionTitleRemote(productId: productId)),
                body: NftPurseReadCopy.description(ownerProductId: ownerProductId),
                icon: makeIcon(systemName: "square.stack.3d.up.fill")
            )
        }
        return PromptContent(
            title: title,
            body: permissionBody(for: permission),
            icon: makeIcon(systemName: permissionIconSystemName(for: permission))
        )
    }

    static func makeBatchedContent(
        productId: String,
        permissions: [ProductPermission]
    ) -> PromptContent {
        let descriptions = permissions.map { permissionDescription(for: $0) }
        let body = descriptions.joined(separator: "\n")
        return PromptContent(
            title: String(localized: .Products.permissionTitleRemote(productId: productId)),
            body: body,
            icon: makeIcon(systemName: "shield.lefthalf.filled")
        )
    }

    static func permissionDescription(for permission: ProductPermission) -> String {
        "- " + permissionBody(for: permission)
    }

    static func permissionBody(for permission: ProductPermission) -> String {
        switch permission {
        case let .networkAccess(domain):
            String(localized: .Products.permissionBodyNetworkAccess(domain: domain))
        case let .networkAccessBundle(domains):
            String(localized: .Products.permissionBodyNetworkAccess(domain: domains.joined(separator: ", ")))
        case .balanceAccess:
            String(localized: .Products.permissionBodyBalanceAccess)
        case .webRtcAccess:
            String(localized: .Products.permissionBodyWebRtc)
        case .chainSubmitAccess:
            String(localized: .Products.permissionBodyChainSubmit)
        case .preimageSubmitAccess:
            String(localized: .Products.permissionBodyPreimageSubmit)
        case .statementSubmitAccess:
            String(localized: .Products.permissionBodyStatementSubmit)
        case let .deviceCapability(capability):
            capabilityDescription(capability)
        case let .accountAccess(targetProductId):
            String(localized: .Products.permissionBodyAccountAccess(targetProductId: targetProductId))
        case .userIdentityAccess:
            String(localized: .Products.permissionBodyUserIdentityAccess)
        case .chatAuthority:
            String(localized: .Products.permissionBodyChatAuthority)
        case .profileDisclosure:
            String(localized: .Products.permissionBodyProfileDisclosure)
        case let .statementStoreAllowance(derivationIndex):
            ProductPermission.statementStoreAllowanceDescription(derivationIndex: derivationIndex)
        case let .nftPurseRead(ownerProductId):
            NftPurseReadCopy.description(ownerProductId: ownerProductId)
        }
    }

    static func permissionIconSystemName(for permission: ProductPermission) -> String {
        switch permission {
        case let .deviceCapability(capability): capabilityIconSystemName(capability)
        case .networkAccess, .networkAccessBundle: "globe"
        case .accountAccess: "person.crop.circle"
        case .balanceAccess: "dollarsign.circle.fill"
        case .webRtcAccess: "video.fill"
        case .chainSubmitAccess: "link"
        case .preimageSubmitAccess: "doc.text"
        case .statementSubmitAccess: "text.bubble"
        case .userIdentityAccess: "person.text.rectangle"
        case .chatAuthority: "message.badge.shield"
        case .profileDisclosure: "person.crop.circle.badge.checkmark"
        case .statementStoreAllowance: "externaldrive.badge.plus"
        case .nftPurseRead: "square.stack.3d.up.fill"
        }
    }

    static func capabilityDescription(_ capability: DeviceCapabilityType) -> String {
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
        case .motion: String(localized: .Products.permissionCapabilityDescriptionMotion)
        }
    }

    static func capabilityIconSystemName(_ capability: DeviceCapabilityType) -> String {
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
        case .motion: "figure.walk.motion"
        }
    }

    static func makeAction(
        title: String,
        handler: @escaping () -> Void
    ) -> MessageSheetAction {
        MessageSheetAction(
            title: LocalizableResource { _ in title },
            handler: handler
        )
    }

    static func capabilityDisplayName(_ capability: DeviceCapabilityType) -> String {
        switch capability {
        case .notifications: String(localized: .Products.permissionCapabilityNotifications)
        case .camera: String(localized: .Products.permissionCapabilityCamera)
        case .microphone: String(localized: .Products.permissionCapabilityMicrophone)
        case .bluetooth: String(localized: .Products.permissionCapabilityBluetooth)
        case .nfc: String(localized: .Products.permissionCapabilityNfc)
        case .location: String(localized: .Products.permissionCapabilityLocation)
        case .clipboard: String(localized: .Products.permissionCapabilityClipboard)
        case .openUrl: String(localized: .Products.permissionCapabilityOpenUrl)
        case .biometrics: String(localized: .Products.permissionCapabilityBiometrics)
        case .motion: String(localized: .Products.permissionCapabilityMotion)
        }
    }

    static func makeIcon(systemName: String) -> UIImage? {
        let config = UIImage.SymbolConfiguration(pointSize: 60, weight: .regular)
        return UIImage(systemName: systemName, withConfiguration: config)?
            .withTintColor(.fgPrimary, renderingMode: .alwaysOriginal)
    }
}

/// What reading another owner's NFT purses reveals, shared by the prompt and the settings list.
enum NftPurseReadCopy {
    static func description(ownerProductId: String?) -> String {
        guard let ownerProductId else {
            return String(localized: .Products.permissionBodyNftPurseReadRoot)
        }
        return String(localized: .Products.permissionBodyNftPurseReadProduct(ownerProductId: ownerProductId))
    }
}
