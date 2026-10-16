import Foundation_iOS
import PolkadotUI
import UIKit
import UIKitExt

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
        case let .preimageSubmit(productId, size):
            title = String(localized: .Products.actionPreimageTitle)
            body = String(localized: .Products.actionPreimageBody(productId: productId, size: size.formatted()))
            iconName = "doc.text"
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
}
