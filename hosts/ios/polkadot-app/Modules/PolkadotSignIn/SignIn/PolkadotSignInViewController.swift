import UIKit
import UIKit_iOS
import PolkadotUI
import FoundationExt

final class PolkadotSignInViewController: UIViewController, ViewHolder {
    typealias RootViewType = PolkadotSignInViewLayout

    let presenter: PolkadotSignInPresenterProtocol

    init(presenter: PolkadotSignInPresenterProtocol) {
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = PolkadotSignInViewLayout()
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        presenter.setup()
        setupHandlers()
    }
}

extension PolkadotSignInViewController: PolkadotSignInViewProtocol {
    func didReceive(viewModel: PolkadotSignInViewLayout.ViewModel) {
        rootView.bind(viewModel: viewModel)
    }
}

extension PolkadotSignInViewController: ModalSheetPresenterDelegate {
    func presenterShouldHide(_: ModalPresenterProtocol) -> Bool {
        false
    }

    func presenterCanDrag(_: ModalPresenterProtocol) -> Bool {
        false
    }
}

private extension PolkadotSignInViewController {
    func setupHandlers() {
        rootView.resultView.linkButton.addTarget(
            self,
            action: #selector(actionApprove),
            for: .touchUpInside
        )

        rootView.resultView.cancelButton.addTarget(
            self,
            action: #selector(actionClose),
            for: .touchUpInside
        )
    }

    @objc
    func actionApprove() {
        presenter.approve()
    }

    @objc
    func actionClose() {
        presenter.cancel()
    }
}
