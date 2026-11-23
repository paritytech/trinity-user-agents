import Foundation
import TrUAPIHost

/// Every provider the core asked, in its order, with its quote or why it
/// would not give one.
final class FundingProvidersPresenter: FundingScreenPresenter<FundingProvidersViewModel> {
    private let wireframe: FundingProvidersWireframeProtocol

    init(
        context: FundingFlowContext,
        interactor: FundingProvidersInteractor,
        wireframe: FundingProvidersWireframeProtocol,
        viewModelFactory: FundingViewModelFactoryProtocol
    ) {
        self.wireframe = wireframe
        super.init(context: context, interactor: interactor) { viewModelFactory.makeProvidersViewModel(for: $0) }
    }
}

extension FundingProvidersPresenter: FundingProvidersPresenterProtocol {
    func chooseProvider(_ providerId: ProductId) {
        context.update { $0.chosenProviderId = providerId }
        wireframe.showToast(String(localized: .Funding.providerChanged))
        wireframe.back()
    }

    func back() {
        wireframe.back()
    }
}
