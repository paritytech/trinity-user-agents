import Foundation

/// Where the user pays from. The core is told of the country's currency, and
/// the providers are asked again for it.
final class FundingCountryPresenter: FundingScreenPresenter {
    weak var view: FundingCountryViewProtocol?
    private let wireframe: FundingCountryWireframeProtocol
    private let viewModelFactory: FundingViewModelFactoryProtocol

    init(
        flowState: FundingFlowState,
        interactor: FundingCountryInteractor,
        wireframe: FundingCountryWireframeProtocol,
        viewModelFactory: FundingViewModelFactoryProtocol
    ) {
        self.wireframe = wireframe
        self.viewModelFactory = viewModelFactory
        super.init(flowState: flowState, interactor: interactor)
    }

    override func render() {
        view?.didReceive(viewModel: viewModelFactory.makeCountryViewModel(for: state))
    }
}

extension FundingCountryPresenter: FundingCountryPresenterProtocol {
    func chooseCountry(_ country: PaymentCountry) {
        flowState.update { draft in
            draft.hasPickedCountry = true
            draft.country = country
        }
        wireframe.back()
        requestQuote()
    }

    func back() {
        wireframe.back()
    }
}
