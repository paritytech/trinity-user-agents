import Foundation_iOS
import FoundationExt
import PolkadotUI
import Products
import SubstrateSdk
import UIKit
import UIKitExt

@MainActor
enum SignVrfPromptViewFactory {
    static func createView(context: SignVrfConfirmationContext) -> ControllerBackedProtocol {
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

private extension SignVrfPromptViewFactory {
    static func makeViewModel(
        for context: SignVrfConfirmationContext
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

    static func makeTitle(for request: SignVrfConfirmationRequest) -> String {
        if let callingProductId = request.callingProductId {
            String(localized: .Products.signVrfTitle(productId: callingProductId))
        } else {
            String(localized: .Products.signVrfTitleUnknownCaller)
        }
    }

    static func makeBody(for request: SignVrfConfirmationRequest) -> String {
        let transcriptLabel = readableText(request.payload.transcriptLabel)

        return if let callingProductId = request.callingProductId {
            String(
                localized: .Products.signVrfBody(
                    transcriptLabel: transcriptLabel,
                    productId: callingProductId
                )
            )
        } else {
            String(localized: .Products.signVrfBodyUnknownCaller(transcriptLabel: transcriptLabel))
        }
    }

    /// Transcript labels are domain-separation strings in practice but opaque bytes on the wire.
    static func readableText(_ data: Data) -> String {
        data.printableUTF8String ?? data.toHex(includePrefix: true)
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
