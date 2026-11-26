import Foundation

/// Which network a crypto session pays on, from those the core says a route
/// takes.
final class FundingNetworkPresenter: FundingScreenPresenter {
    weak var view: FundingNetworkViewProtocol?
    private let wireframe: FundingNetworkWireframeProtocol
    private let viewModelFactory: FundingViewModelFactoryProtocol

    init(
        flowState: FundingFlowState,
        interactor: FundingNetworkInteractor,
        wireframe: FundingNetworkWireframeProtocol,
        viewModelFactory: FundingViewModelFactoryProtocol
    ) {
        self.wireframe = wireframe
        self.viewModelFactory = viewModelFactory
        super.init(flowState: flowState, interactor: interactor)
    }

    override func render() {
        view?.didReceive(viewModel: viewModelFactory.makeNetworkViewModel(for: model))
    }
}

extension FundingNetworkPresenter: FundingNetworkPresenterProtocol {
    func chooseNetwork(_ id: String) {
        flowState.update { draft in
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
