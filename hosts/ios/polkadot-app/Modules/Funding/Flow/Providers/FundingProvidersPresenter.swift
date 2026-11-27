import Foundation
import TrUAPIHost

/// Every provider the core asked, in its order, with its quote or why it
/// would not give one.
final class FundingProvidersPresenter: FundingScreenPresenter {
    weak var view: FundingProvidersViewProtocol?
    private let wireframe: FundingProvidersWireframeProtocol
    private let viewModelFactory: FundingViewModelFactoryProtocol

    init(
        flowState: FundingFlowState,
        interactor: FundingProvidersInteractor,
        wireframe: FundingProvidersWireframeProtocol,
        viewModelFactory: FundingViewModelFactoryProtocol
    ) {
        self.wireframe = wireframe
        self.viewModelFactory = viewModelFactory
        super.init(flowState: flowState, interactor: interactor)
    }

    override func render() {
        view?.didReceive(viewModel: viewModelFactory.makeProvidersViewModel(for: model))
    }
}

extension FundingProvidersPresenter: FundingProvidersPresenterProtocol {
    func chooseProvider(_ providerId: ProductId) {
        flowState.update { $0.chosenProviderId = providerId }
        wireframe.showToast(String(localized: .Funding.providerChanged))
        wireframe.back()
    }

    func back() {
        wireframe.back()
    }
}
