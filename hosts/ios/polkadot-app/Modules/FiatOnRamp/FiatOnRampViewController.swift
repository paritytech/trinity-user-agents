import Foundation_iOS
import PolkadotUI
import SwiftUI
import UIKit

final class FiatOnRampViewController: UIHostingController<FiatOnRampViewLayout> {
    let presenter: FiatOnRampPresenterProtocol
    private let viewModel = FiatOnRampViewModel()

    init(presenter: FiatOnRampPresenterProtocol) {
        self.presenter = presenter
        super.init(rootView: FiatOnRampViewLayout(viewModel: viewModel))
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .bgSurfaceMain

        navigationItem.title = String(localized: .fiatOnrampAmountTitle)
        setupHandlers()
        presenter.setup()
    }

    private func setupHandlers() {
        viewModel.onAmountChanged = { [unowned presenter] amount in
            presenter.onAmountChanged(amount)
        }

        viewModel.onContinue = { [unowned presenter] amount in
            presenter.onContinue(amount: amount)
        }

        viewModel.onSelectQuickAmount = { [unowned presenter] quickAmount in
            presenter.onSelectQuickAmount(quickAmount)
        }
    }
}

extension FiatOnRampViewController: FiatOnRampViewProtocol {
    func didReceive(quickAmounts: [FiatOnRampQuickAmountViewModel]) {
        viewModel.quickAmounts = quickAmounts
    }

    func didReceive(amount: Int?) {
        viewModel.amount = amount
    }

    func didReceive(amountError: String?) {
        viewModel.amountError = amountError
    }
}
