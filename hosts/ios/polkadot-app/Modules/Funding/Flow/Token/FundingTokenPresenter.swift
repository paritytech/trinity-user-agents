import Foundation

/// Which token a crypto session pays in or receives; the summary shows the
/// provider, fees and total before the user starts.
final class FundingTokenPresenter: FundingScreenPresenter {
    weak var view: FundingTokenViewProtocol?
    private let wireframe: FundingTokenWireframeProtocol
    private let viewModelFactory: FundingViewModelFactoryProtocol

    init(
        context: FundingFlowContext,
        interactor: FundingTokenInteractor,
        wireframe: FundingTokenWireframeProtocol,
        viewModelFactory: FundingViewModelFactoryProtocol
    ) {
        self.wireframe = wireframe
        self.viewModelFactory = viewModelFactory
        super.init(context: context, interactor: interactor)
    }

    override func render() {
        view?.didReceive(viewModel: viewModelFactory.makeTokenViewModel(for: state))
    }
}

extension FundingTokenPresenter: FundingTokenPresenterProtocol {
    func chooseToken(_ symbol: String) {
        context.update { $0.asset = symbol }
        requestQuote()
        wireframe.show(.summary)
    }

    func back() {
        wireframe.back()
    }
}
