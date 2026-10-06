import Foundation_iOS
import PolkadotUI
import UIKit
import UIKitExt
import SubstrateSdk

@MainActor
enum TrUAPIActionPromptViewFactory {
    static func createView(context: TrUAPIActionConfirmationContext) -> ControllerBackedProtocol {
        let view = TitleDetailsSheetViewFactory.createView(
            from: makeViewModel(for: context),
            styler: ProductPromptStyler(),
            allowsSwipeDown: false
        )
        BottomSheetViewFacade.setupBottomSheet(from: view.controller)
        return view
    }

    static func makeViewModel(for context: TrUAPIActionConfirmationContext) -> TitleDetailsSheetViewModel {
        let title: String
        let body: String
        let iconName: String
        switch context.request {
        case let .productSubtree(productId):
            title = String(localized: .Products.actionProductSubtreeTitle)
            body = String(localized: .Products.actionProductSubtreeBody(productId: productId))
            iconName = "person.crop.circle"
        }

        let icon = UIImage(systemName: iconName, withConfiguration: UIImage.SymbolConfiguration(
            pointSize: 60, weight: .regular
        ))?.withTintColor(.fgPrimary, renderingMode: .alwaysOriginal)

        return TitleDetailsSheetViewModel(
            graphics: icon,
            title: LocalizableResource { _ in title },
            message: LocalizableResource { _ in .normal(body) },
            mainAction: MessageSheetAction(
                title: LocalizableResource { _ in String(localized: .Common.confirm) },
                handler: { context.deliver(true) }
            ),
            secondaryAction: MessageSheetAction(
                title: LocalizableResource { _ in String(localized: .Common.reject) },
                handler: { context.deliver(false) }
            )
        )
    }

    static func createPreimageView(context: TrUAPIPreimageConfirmationContext) -> ControllerBackedProtocol {
        let view = TitleDetailsSheetViewFactory.createView(
            from: makePreimageViewModel(for: context),
            styler: ProductPromptStyler(),
            allowsSwipeDown: false
        )
        BottomSheetViewFacade.setupBottomSheet(from: view.controller)
        return view
    }

    static func makePreimageViewModel(for context: TrUAPIPreimageConfirmationContext) -> TitleDetailsSheetViewModel {
        let review = context.review
        let body = String(localized: .Products.actionPreimageConsentBody(
            productId: review.productId,
            size: review.size.formatted(),
            rootAccount: review.rootPublicKey.toHex(includePrefix: true),
            bulletinNetwork: review.genesisHash.toHex(includePrefix: true),
            maxBytes: review.automaticMaxBytes.formatted(),
            maxUploads: review.automaticMaxUploads.formatted(),
            windowSeconds: review.automaticWindowSeconds.formatted()
        ))
        return TitleDetailsSheetViewModel(
            graphics: UIImage(systemName: "doc.text", withConfiguration: UIImage.SymbolConfiguration(
                pointSize: 60, weight: .regular
            ))?.withTintColor(.fgPrimary, renderingMode: .alwaysOriginal),
            title: LocalizableResource { _ in String(localized: .Products.actionPreimageTitle) },
            message: LocalizableResource { _ in .normal(body) },
            mainAction: MessageSheetAction(
                title: LocalizableResource { _ in String(localized: .Products.permissionActionAllowOnce) },
                handler: { context.deliver(.allowOnce) }
            ),
            secondaryAction: MessageSheetAction(
                title: LocalizableResource { _ in String(localized: .Products.actionPreimageAllowAutomatic) },
                handler: { context.deliver(.allowAlways) }
            ),
            tertiaryAction: MessageSheetAction(
                title: LocalizableResource { _ in String(localized: .Products.permissionActionDeny) },
                handler: { context.deliver(.deny) }
            )
        )
    }
}
