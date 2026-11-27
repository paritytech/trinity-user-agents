import UIKit

final class FundingSummaryViewController: FundingScreenController<FundingSummaryView> {
    private let presenter: FundingSummaryPresenterProtocol

    init(presenter: FundingSummaryPresenterProtocol) {
        self.presenter = presenter
        super.init(screen: .summary, onLoad: presenter.setup)
    }

    @available(*, unavailable)
    @MainActor dynamic required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

extension FundingSummaryViewController: FundingSummaryViewProtocol {
    func didReceive(viewModel: FundingSummaryViewModel) {
        rootView = FundingSummaryView(viewModel: viewModel, presenter: presenter)
    }
}
