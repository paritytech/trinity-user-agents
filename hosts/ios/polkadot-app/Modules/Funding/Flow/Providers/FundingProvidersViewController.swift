import UIKit

final class FundingProvidersViewController: FundingScreenController<FundingProviderListView> {
    private let presenter: FundingProvidersPresenterProtocol

    init(presenter: FundingProvidersPresenterProtocol) {
        self.presenter = presenter
        super.init(screen: .providers, onLoad: presenter.setup)
    }

    @available(*, unavailable)
    @MainActor dynamic required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

extension FundingProvidersViewController: FundingProvidersViewProtocol {
    func didReceive(viewModel: FundingProvidersViewModel) {
        rootView = FundingProviderListView(viewModel: viewModel, presenter: presenter)
    }
}
