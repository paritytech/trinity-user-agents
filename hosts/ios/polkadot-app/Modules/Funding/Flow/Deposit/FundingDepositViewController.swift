import UIKit

final class FundingDepositViewController: FundingScreenController<FundingDepositView> {
    private let presenter: FundingDepositPresenterProtocol

    init(presenter: FundingDepositPresenterProtocol) {
        self.presenter = presenter
        super.init(screen: .deposit, onLoad: presenter.setup)
    }

    @available(*, unavailable)
    @MainActor dynamic required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

extension FundingDepositViewController: FundingDepositViewProtocol {
    func didReceive(viewModel: FundingDepositViewModel) {
        rootView = FundingDepositView(viewModel: viewModel, presenter: presenter)
    }
}
