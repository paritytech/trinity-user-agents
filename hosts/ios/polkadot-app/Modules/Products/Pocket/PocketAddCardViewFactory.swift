import Products
import SwiftUI
import UIKit
import UIKitExt

@MainActor
enum PocketAddCardViewFactory {
    /// Nil when the Pocket cannot be read yet, which is also when there is
    /// nowhere to put an approved card.
    static func createView(
        for link: PocketDeeplink,
        flowState: SPAFlowState,
        pocket: ProductPocketService
    ) -> UIViewController? {
        let viewModel = PocketAddCardViewModel(
            productId: link.productHost,
            cardId: link.cardId,
            interactor: makeInteractor(store: pocket.collection, flowState: flowState),
            images: { pocket.images(of: $0) }
        )

        let controller = UIHostingController(rootView: PocketAddCardView(viewModel: viewModel))
        controller.view.backgroundColor = .bgSurfaceMain
        viewModel.onFinish = { [weak controller] in controller?.dismiss(animated: true) }

        // Sized to the card rather than to the screen: this asks for one
        // decision, and what is behind it stays visible.
        BottomSheetViewFacade.setupBottomSheet(
            from: controller,
            preferredHeight: PocketAddCardView.sheetHeight
        )

        return controller
    }

    private static func makeInteractor(
        store: any PocketCardStore,
        flowState: SPAFlowState
    ) -> PocketAddCardInteractor {
        PocketAddCardInteractor(
            publishedCards: PublishedPocketCards.makeDefault(products: flowState.productResolver),
            previews: PocketPreviewLoader(
                archive: ProductWorkerArchive(dotNsResolver: flowState.dotNsResolver, content: .current),
                fetch: PocketPreviewFetch.bounded
            ),
            store: store
        )
    }
}
