import UIKit
import PolkadotUI
import UIKit_iOS
import FoundationExt

final class TransferPrivacyViewController: UIViewController, ViewHolder {
    typealias RootViewType = TransferPrivacyViewLayout

    let presenter: TransferPrivacyPresenterProtocol

    init(presenter: TransferPrivacyPresenterProtocol) {
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = RootViewType()
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        setupActions()
        presenter.setup()
    }

    private func setupActions() {
        let sendAnywayAction = UIAction { [weak presenter] _ in
            presenter?.sendAnyway()
        }
        rootView.mainButton.addAction(sendAnywayAction, for: .touchUpInside)
        let cancelAction = UIAction { [weak presenter] _ in
            presenter?.cancel()
        }
        rootView.cancelButton.addAction(cancelAction, for: .touchUpInside)
    }
}

extension TransferPrivacyViewController: TransferPrivacyViewProtocol {
    func didReceive(viewModel: TransferPrivacyViewModel) {
        rootView.bind(viewModel: viewModel)
    }
}

/// The decision is taken with the buttons only: a backdrop tap or a swipe does not close the sheet.
extension TransferPrivacyViewController: ModalPresenterDelegate {
    func presenterShouldHide(_: ModalPresenterProtocol) -> Bool {
        false
    }

    func presenterDidHide(_: ModalPresenterProtocol) {}
}
