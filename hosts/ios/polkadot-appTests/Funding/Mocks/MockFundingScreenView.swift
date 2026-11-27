import UIKit
@testable import polkadot_app

/// Every funding screen's view, keeping the latest view model each screen's
/// presenter pushed.
@MainActor
final class MockFundingScreenView {
    private(set) var amount: FundingAmountViewModel?
    private(set) var summary: FundingSummaryViewModel?
    private(set) var country: FundingCountryViewModel?
    private(set) var network: FundingNetworkViewModel?
    private(set) var token: FundingTokenViewModel?
    private(set) var providers: FundingProvidersViewModel?
    private(set) var deposit: FundingDepositViewModel?
    private(set) var progress: FundingProgressViewModel?
    private(set) var cancelConfirm: FundingCancelConfirmViewModel?

    let changes = StateChangeSignal()

    let isSetup = true
    let controller = UIViewController()
}

extension MockFundingScreenView: FundingAmountViewProtocol {
    func didReceive(viewModel: FundingAmountViewModel) {
        amount = viewModel
        changes.signal()
    }
}

extension MockFundingScreenView: FundingSummaryViewProtocol {
    func didReceive(viewModel: FundingSummaryViewModel) {
        summary = viewModel
        changes.signal()
    }
}

extension MockFundingScreenView: FundingCountryViewProtocol {
    func didReceive(viewModel: FundingCountryViewModel) {
        country = viewModel
        changes.signal()
    }
}

extension MockFundingScreenView: FundingNetworkViewProtocol {
    func didReceive(viewModel: FundingNetworkViewModel) {
        network = viewModel
        changes.signal()
    }
}

extension MockFundingScreenView: FundingTokenViewProtocol {
    func didReceive(viewModel: FundingTokenViewModel) {
        token = viewModel
        changes.signal()
    }
}

extension MockFundingScreenView: FundingProvidersViewProtocol {
    func didReceive(viewModel: FundingProvidersViewModel) {
        providers = viewModel
        changes.signal()
    }
}

extension MockFundingScreenView: FundingDepositViewProtocol {
    func didReceive(viewModel: FundingDepositViewModel) {
        deposit = viewModel
        changes.signal()
    }
}

extension MockFundingScreenView: FundingProgressViewProtocol {
    func didReceive(viewModel: FundingProgressViewModel) {
        progress = viewModel
        changes.signal()
    }
}

extension MockFundingScreenView: FundingCancelConfirmViewProtocol {
    func didReceive(viewModel: FundingCancelConfirmViewModel) {
        cancelConfirm = viewModel
        changes.signal()
    }
}
