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
        switch permission {
        case let .deviceCapability(capability):
            PromptContent(
                title: String(
                    localized: .Products.permissionTitleDeviceCapability(
                        productId: productId,
                        capability: capabilityDisplayName(capability)
                    )
                ),
                body: capabilityDescription(capability),
                icon: iconForCapability(capability)
            )
        case let .networkAccess(domain):
            PromptContent(
                title: String(
                    localized: .Products.permissionTitleNetworkAccess(
                        productId: productId
                    )
                ),
                body: String(
                    localized: .Products.permissionBodyNetworkAccess(domain: domain)
                ),
                icon: makeIcon(systemName: "globe")
            )
        case let .accountAccess(targetProductId):
            PromptContent(
                title: String(
                    localized: .Products.permissionTitleAccountAccess(
                        productId: productId
                    )
                ),
                body: String(
                    localized: .Products.permissionBodyAccountAccess(
                        targetProductId: targetProductId
                    )
                ),
                icon: makeIcon(systemName: "person.crop.circle")
            )
        case .balanceAccess:
            PromptContent(
                title: String(localized: .Products.permissionTitleRemote(productId: productId)),
                body: String(localized: .Products.permissionBodyBalanceAccess),
                icon: makeIcon(systemName: "dollarsign.circle.fill")
            )
        case .webRtcAccess:
            PromptContent(
                title: String(localized: .Products.permissionTitleRemote(productId: productId)),
                body: String(localized: .Products.permissionBodyWebRtc),
                icon: makeIcon(systemName: "video.fill")
            )
        case .chainSubmitAccess:
            PromptContent(
                title: String(localized: .Products.permissionTitleRemote(productId: productId)),
                body: String(localized: .Products.permissionBodyChainSubmit),
                icon: makeIcon(systemName: "link")
            )
        case .preimageSubmitAccess:
            PromptContent(
                title: String(localized: .Products.permissionTitleRemote(productId: productId)),
                body: String(localized: .Products.permissionBodyPreimageSubmit),
                icon: makeIcon(systemName: "doc.text")
            )
        case .statementSubmitAccess:
            PromptContent(
                title: String(localized: .Products.permissionTitleRemote(productId: productId)),
                body: String(localized: .Products.permissionBodyStatementSubmit),
                icon: makeIcon(systemName: "text.bubble")
            )
        case .userIdentityAccess:
            PromptContent(
                title: String(localized: .Products.permissionTitleRemote(productId: productId)),
                body: String(localized: .Products.permissionBodyUserIdentityAccess),
                icon: makeIcon(systemName: "person.text.rectangle")
            )
        }
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
        switch permission {
        case let .networkAccess(domain):
            "- " + String(
                localized: .Products.permissionBodyNetworkAccess(domain: domain)
            )
        case .balanceAccess:
            "- " + String(localized: .Products.permissionBodyBalanceAccess)
        case .webRtcAccess:
            "- " + String(localized: .Products.permissionBodyWebRtc)
        case .chainSubmitAccess:
            "- " + String(localized: .Products.permissionBodyChainSubmit)
        case .preimageSubmitAccess:
            "- " + String(localized: .Products.permissionBodyPreimageSubmit)
        case .statementSubmitAccess:
            "- " + String(localized: .Products.permissionBodyStatementSubmit)
        case let .deviceCapability(capability):
            "- " + capabilityDescription(capability)
        case let .accountAccess(targetProductId):
            "- " + String(
                localized: .Products.permissionBodyAccountAccess(
                    targetProductId: targetProductId
                )
            )
        case .userIdentityAccess:
            "- " + String(localized: .Products.permissionBodyUserIdentityAccess)
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
        }
    }

    static func iconForCapability(_ capability: DeviceCapabilityType) -> UIImage? {
        let name =
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
        return makeIcon(systemName: name)
    }

    static func makeIcon(systemName: String) -> UIImage? {
        let config = UIImage.SymbolConfiguration(pointSize: 60, weight: .regular)
        return UIImage(systemName: systemName, withConfiguration: config)?
            .withTintColor(.fgPrimary, renderingMode: .alwaysOriginal)
    }
}
