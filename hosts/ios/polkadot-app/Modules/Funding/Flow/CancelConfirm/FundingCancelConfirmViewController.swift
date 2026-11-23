import UIKit

final class FundingCancelConfirmViewController: FundingScreenController<FundingCancelConfirmView> {
    private let presenter: FundingCancelConfirmPresenterProtocol

    init(presenter: FundingCancelConfirmPresenterProtocol) {
        self.presenter = presenter
        super.init(screen: .cancelConfirm, onLoad: presenter.setup)
    }

    @available(*, unavailable)
    @MainActor dynamic required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

extension FundingCancelConfirmViewController: FundingCancelConfirmViewProtocol {
    func didReceive(viewModel: FundingCancelConfirmViewModel) {
        rootView = FundingCancelConfirmView(viewModel: viewModel, presenter: presenter)
    }
}
