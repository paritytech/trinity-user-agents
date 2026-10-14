import UIKit
import PolkadotUI
import SwiftUI

final class DepositViewController: UIHostingController<DepositViewLayout> {
    let presenter: DepositPresenterProtocol

    init(presenter: DepositPresenterProtocol) {
        self.presenter = presenter
        super.init(rootView: DepositViewLayout())
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .bgSurfaceMain
        setupActions()
        setupDoneItem()

        presenter.setup()
    }
}

private extension DepositViewController {
    func updateDoneState(_ shouldDisable: Bool) {
        navigationItem.rightBarButtonItem?.isEnabled = !shouldDisable
        navigationItem.rightBarButtonItem?.tintColor = shouldDisable ? .textAndIconsDisabled : .white100
    }
}

extension DepositViewController: DepositViewProtocol {
    func didReceive(assetsViewModel: DepositAssetsViewModel) {
        rootView.viewModel.assetsViewModel = assetsViewModel
    }

    func didReceive(summaryViewModel: DepositSummaryViewModel) {
        rootView.viewModel.summaryViewModel = summaryViewModel
    }

    func didReceive(operationsViewModel: [DepositOperationViewModel]) {
        let shouldDisableDone = operationsViewModel.contains(where: \.isInProgress)

        // Prevent interactive dismissal
        isModalInPresentation = shouldDisableDone
        updateDoneState(shouldDisableDone)

        rootView.viewModel.operationsViewModel = operationsViewModel
    }
}

private extension DepositViewController {
    func setupActions() {
        rootView.viewModel.onCopyAddress = { [weak self] in
            self?.presenter.copyAddress()
        }
    }

    func setupDoneItem() {
        navigationItem.rightBarButtonItem = UIBarButtonItem(
            barButtonSystemItem: .done,
            target: self,
            action: #selector(didTapDoneItem)
        )
    }

    @objc
    func didTapDoneItem() {
        presenter.done()
    }
}
