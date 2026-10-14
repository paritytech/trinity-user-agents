import Foundation_iOS
import FoundationExt
import PolkadotUI
import SubstrateSdk
import UIKit
import UIKitExt

@MainActor
enum CreateProofPromptViewFactory {
    static func createView(context: CreateProofConfirmationContext) -> ControllerBackedProtocol {
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

private extension CreateProofPromptViewFactory {
    static func makeViewModel(
        for context: CreateProofConfirmationContext
    ) -> TitleDetailsSheetViewModel {
        let request = context.request

        return TitleDetailsSheetViewModel(
            graphics: makeIcon(),
            title: LocalizableResource { _ in makeTitle(for: request) },
            message: LocalizableResource { _ in .normal(makeBody(for: request)) },
            mainAction: makeAction(
                title: String(localized: .Common.approve)
            ) { context.deliver(.approved) },
            secondaryAction: makeAction(
                title: String(localized: .Common.reject)
            ) { context.deliver(.rejected) }
        )
    }

    static func makeTitle(for request: CreateProofConfirmationRequest) -> String {
        if let callingProductId = request.callingProductId {
            String(
                localized: .Products.createProofTitle(
                    productId: callingProductId,
                    targetProductId: request.onBehalfOfProductId
                )
            )
        } else {
            String(localized: .Products.createProofTitleUnknownCaller)
        }
    }

    static func makeBody(for request: CreateProofConfirmationRequest) -> String {
        [
            String(
                localized: .Products.createProofBodyOnBehalfOf(
                    targetProductId: request.onBehalfOfProductId
                )
            ),
            String(
                localized: .Products.createProofBodyContext(
                    context: request.suffix.toHex(includePrefix: true)
                )
            ),
            String(
                localized: .Products.createProofBodyMessage(
                    message: request.message.printableUTF8String
                        ?? request.message.toHex(includePrefix: true)
                )
            )
        ]
        .joined(separator: "\n")
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
        return UIImage(systemName: "person.badge.shield.checkmark", withConfiguration: config)?
            .withTintColor(.fgPrimary, renderingMode: .alwaysOriginal)
    }
}
