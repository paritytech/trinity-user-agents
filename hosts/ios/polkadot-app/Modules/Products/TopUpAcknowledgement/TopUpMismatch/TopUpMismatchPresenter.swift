import Foundation
import PolkadotUI
import Products
import SubstrateSdk

/// Shows the credited amount against the requested one when a top-up settled partially. Purely
/// informational — closing simply dismisses (the `paymentTopUp` host call returned long ago; the
/// product already learned the outcome via the status subscription).
final class TopUpMismatchPresenter {
    weak var view: TopUpMismatchViewProtocol?
    let wireframe: TopUpMismatchWireframeProtocol

    private let productId: ProductId
    private let claimedAmount: Balance
    private let requestedAmount: Balance
    private let viewModelFactory: TopUpAcknowledgementViewModelMaking

    init(
        wireframe: TopUpMismatchWireframeProtocol,
        productId: ProductId,
        claimedAmount: Balance,
        requestedAmount: Balance,
        viewModelFactory: TopUpAcknowledgementViewModelMaking
    ) {
        self.wireframe = wireframe
        self.productId = productId
        self.claimedAmount = claimedAmount
        self.requestedAmount = requestedAmount
        self.viewModelFactory = viewModelFactory
    }
}

extension TopUpMismatchPresenter: TopUpMismatchPresenterProtocol {
    func setup() {
        let viewModel = TopUpMismatchViewModel(
            title: viewModelFactory.amountMismatchTitle(productId: productId),
            claimedAmount: viewModelFactory.formatAmountValue(claimedAmount),
            originalAmount: viewModelFactory.formatAmountValue(requestedAmount),
            tokenSymbol: viewModelFactory.tokenSymbol(),
            subtitle: viewModelFactory.amountMismatchWarning(),
            closeButtonTitle: String(localized: .Common.close)
        )
        viewModel.onCloseTapped = { [weak self] in
            self?.didTapClose()
        }
        view?.didReceive(viewModel: viewModel)
    }

    func didTapClose() {
        wireframe.dismiss(view: view)
    }
}
