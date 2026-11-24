import UIKit

final class FundingProgressViewController: FundingScreenController<FundingProgressView> {
    private let presenter: FundingProgressPresenterProtocol
    private let branding: FundingProviderBranding

    init(presenter: FundingProgressPresenterProtocol, branding: FundingProviderBranding) {
        self.presenter = presenter
        self.branding = branding
        super.init(screen: .progress, onLoad: presenter.setup)
    }

    @available(*, unavailable)
    @MainActor dynamic required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

extension FundingProgressViewController: FundingProgressViewProtocol {
    func didReceive(viewModel: FundingProgressViewModel) {
        rootView = FundingProgressView(viewModel: viewModel, presenter: presenter, branding: branding)
    }
}
