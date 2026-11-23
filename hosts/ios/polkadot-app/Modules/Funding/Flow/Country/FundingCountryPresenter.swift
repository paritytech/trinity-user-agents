import Foundation

/// Where the user pays from. The core is told of the country's currency, and
/// the providers are asked again for it.
final class FundingCountryPresenter: FundingScreenPresenter<FundingCountryViewModel> {
    private let wireframe: FundingCountryWireframeProtocol

    init(
        context: FundingFlowContext,
        interactor: FundingCountryInteractor,
        wireframe: FundingCountryWireframeProtocol,
        viewModelFactory: FundingViewModelFactoryProtocol
    ) {
        self.wireframe = wireframe
        super.init(context: context, interactor: interactor) { viewModelFactory.makeCountryViewModel(for: $0) }
    }
}

extension FundingCountryPresenter: FundingCountryPresenterProtocol {
    func chooseCountry(_ country: PaymentCountry) {
        context.update { draft in
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
