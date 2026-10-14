import UIKit
import UIKit_iOS
import PolkadotUI
import FoundationExt

final class PolkadotSigningViewController: UIViewController, ViewHolder {
    typealias RootViewType = PolkadotSigningViewLayout

    let presenter: PolkadotSigningPresenterProtocol

    init(presenter: PolkadotSigningPresenterProtocol) {
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = PolkadotSigningViewLayout()
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        presenter.setup()
        setupHandlers()
    }
}

extension PolkadotSigningViewController: PolkadotSigningViewProtocol {
    func didReceive(viewModel: PolkadotSigningViewLayout.ViewModel) {
        rootView.bind(viewModel: viewModel)
    }
}

extension PolkadotSigningViewController: ModalSheetPresenterDelegate {
    func presenterShouldHide(_: ModalPresenterProtocol) -> Bool {
        false
    }

    func presenterCanDrag(_: ModalPresenterProtocol) -> Bool {
        false
    }
}

private extension PolkadotSigningViewController {
    func setupHandlers() {
        rootView.resultView.signButton.addTarget(
            self,
            action: #selector(actionSign),
            for: .touchUpInside
        )

        rootView.resultView.rejectButton.addTarget(
            self,
            action: #selector(actionClose),
            for: .touchUpInside
        )

        rootView.failureCloseButton.addTarget(
            self,
            action: #selector(actionClose),
            for: .touchUpInside
        )

        rootView.resultView.viewDetailsButton.addTarget(
            self,
            action: #selector(actionViewDetails),
            for: .touchUpInside
        )
    }

    @objc
    func actionSign() {
        presenter.sign()
    }

    @objc
    func actionClose() {
        presenter.cancel()
    }

    @objc
    func actionViewDetails() {
        presenter.viewDetails()
    }
}
