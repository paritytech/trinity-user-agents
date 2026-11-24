import UIKit

final class FundingCountryViewController: FundingScreenController<FundingCountryView> {
    private let presenter: FundingCountryPresenterProtocol

    init(presenter: FundingCountryPresenterProtocol) {
        self.presenter = presenter
        super.init(screen: .country, onLoad: presenter.setup)
    }

    @available(*, unavailable)
    @MainActor dynamic required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

extension FundingCountryViewController: FundingCountryViewProtocol {
    func didReceive(viewModel: FundingCountryViewModel) {
        rootView = FundingCountryView(viewModel: viewModel, presenter: presenter)
    }
}
