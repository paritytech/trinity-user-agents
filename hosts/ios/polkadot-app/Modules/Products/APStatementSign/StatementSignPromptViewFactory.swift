import Foundation_iOS
import FoundationExt
import PolkadotUI
import UIKit
import UIKitExt

@MainActor
enum StatementSignPromptViewFactory {
    static func createView(context: StatementSignConfirmationContext) -> ControllerBackedProtocol {
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

// MARK: - ViewModel Building

private extension StatementSignPromptViewFactory {
    static func makeViewModel(
        for context: StatementSignConfirmationContext
    ) -> TitleDetailsSheetViewModel {
        let request = context.request

        return TitleDetailsSheetViewModel(
            graphics: makeIcon(),
            title: LocalizableResource { _ in
                String(localized: .Products.statementSignTitle)
            },
            message: LocalizableResource { _ in
                .normal(String(localized: .Products.statementSignBody(productId: request.productId)))
            },
            mainAction: makeAction(
                title: String(localized: .Common.confirm)
            ) { context.deliver(.approved) },
            secondaryAction: makeAction(
                title: String(localized: .Common.reject)
            ) { context.deliver(.rejected) }
        )
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

    static func makeIcon() -> UIImage? {
        let config = UIImage.SymbolConfiguration(pointSize: 60, weight: .regular)
        return UIImage(systemName: "signature", withConfiguration: config)?
            .withTintColor(.fgPrimary, renderingMode: .alwaysOriginal)
    }
}
