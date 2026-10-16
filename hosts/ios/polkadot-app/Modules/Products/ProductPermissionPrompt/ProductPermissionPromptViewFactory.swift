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
        case .statementStoreAllowance:
            title = String(localized: .Products.permissionTitleStatementStoreAllowance(productId: productId))
        }
        return PromptContent(
            title: title,
            body: permission.permissionDescription,
            icon: makeIcon(systemName: permission.permissionIconSystemName)
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
        "- " + permission.permissionDescription
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

    static func makeIcon(systemName: String) -> UIImage? {
        let config = UIImage.SymbolConfiguration(pointSize: 60, weight: .regular)
        return UIImage(systemName: systemName, withConfiguration: config)?
            .withTintColor(.fgPrimary, renderingMode: .alwaysOriginal)
    }
}
