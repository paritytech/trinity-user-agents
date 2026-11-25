import UIKit

final class FundingFeesViewController: FundingScreenController<FundingFeesView> {
    private let presenter: FundingFeesPresenterProtocol

    init(presenter: FundingFeesPresenterProtocol) {
        self.presenter = presenter
        super.init(screen: .fees, onLoad: presenter.setup)
    }

    @available(*, unavailable)
    @MainActor dynamic required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

extension FundingFeesViewController: FundingFeesViewProtocol {
    func didReceive(viewModel: FundingFeesViewModel) {
        rootView = FundingFeesView(viewModel: viewModel, presenter: presenter)
    }
}
