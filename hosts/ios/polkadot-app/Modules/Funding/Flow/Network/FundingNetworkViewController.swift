import UIKit

final class FundingNetworkViewController: FundingScreenController<FundingNetworkView> {
    private let presenter: FundingNetworkPresenterProtocol

    init(presenter: FundingNetworkPresenterProtocol) {
        self.presenter = presenter
        super.init(screen: .network, onLoad: presenter.setup)
    }

    @available(*, unavailable)
    @MainActor dynamic required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

extension FundingNetworkViewController: FundingNetworkViewProtocol {
    func didReceive(viewModel: FundingNetworkViewModel) {
        rootView = FundingNetworkView(viewModel: viewModel, presenter: presenter)
    }
}
