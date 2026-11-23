import Foundation

/// What the quote the session is on charges, line by line.
final class FundingFeesPresenter: FundingScreenPresenter {
    weak var view: FundingFeesViewProtocol?
    private let wireframe: FundingFeesWireframeProtocol
    private let viewModelFactory: FundingViewModelFactoryProtocol

    init(
        context: FundingFlowContext,
        interactor: FundingFeesInteractor,
        wireframe: FundingFeesWireframeProtocol,
        viewModelFactory: FundingViewModelFactoryProtocol
    ) {
        self.wireframe = wireframe
        self.viewModelFactory = viewModelFactory
        super.init(context: context, interactor: interactor)
    }

    override func render() {
        view?.didReceive(viewModel: viewModelFactory.makeFeesViewModel(for: state))
    }
}

extension FundingFeesPresenter: FundingFeesPresenterProtocol {
    func back() {
        wireframe.back()
    }
}
