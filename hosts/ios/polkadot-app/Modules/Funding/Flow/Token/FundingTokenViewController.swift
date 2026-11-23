import UIKit

final class FundingTokenViewController: FundingScreenController<FundingTokenView> {
    private let presenter: FundingTokenPresenterProtocol

    init(presenter: FundingTokenPresenterProtocol) {
        self.presenter = presenter
        super.init(screen: .token, onLoad: presenter.setup)
    }

    @available(*, unavailable)
    @MainActor dynamic required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

extension FundingTokenViewController: FundingTokenViewProtocol {
    func didReceive(viewModel: FundingTokenViewModel) {
        rootView = FundingTokenView(viewModel: viewModel, presenter: presenter)
    }
}
