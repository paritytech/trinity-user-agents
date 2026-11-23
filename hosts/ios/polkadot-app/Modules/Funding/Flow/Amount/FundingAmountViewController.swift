import UIKit

final class FundingAmountViewController: FundingScreenController<FundingAmountView> {
    private let presenter: FundingAmountPresenterProtocol

    init(presenter: FundingAmountPresenterProtocol) {
        self.presenter = presenter
        super.init(screen: .amount, onLoad: presenter.setup)
    }

    @available(*, unavailable)
    @MainActor dynamic required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

extension FundingAmountViewController: FundingAmountViewProtocol {
    func didReceive(viewModel: FundingAmountViewModel) {
        rootView = FundingAmountView(viewModel: viewModel, presenter: presenter)
    }
}
