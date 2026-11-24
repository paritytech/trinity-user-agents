import Foundation

/// Which network a crypto session pays on, from those the core says a route
/// takes.
final class FundingNetworkPresenter: FundingScreenPresenter {
    weak var view: FundingNetworkViewProtocol?
    private let wireframe: FundingNetworkWireframeProtocol
    private let viewModelFactory: FundingViewModelFactoryProtocol

    init(
        context: FundingFlowContext,
        interactor: FundingNetworkInteractor,
        wireframe: FundingNetworkWireframeProtocol,
        viewModelFactory: FundingViewModelFactoryProtocol
    ) {
        self.wireframe = wireframe
        self.viewModelFactory = viewModelFactory
        super.init(context: context, interactor: interactor)
    }

    override func render() {
        view?.didReceive(viewModel: viewModelFactory.makeNetworkViewModel(for: state))
    }
}

extension FundingNetworkPresenter: FundingNetworkPresenterProtocol {
    func chooseNetwork(_ id: String) {
        context.update { draft in
            draft.network = id
            draft.asset = nil
        }
        reportSelection()
        wireframe.show(.token)
    }

    func back() {
        wireframe.back()
    }
}
