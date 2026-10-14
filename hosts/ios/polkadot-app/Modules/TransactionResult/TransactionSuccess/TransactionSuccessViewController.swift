import UIKit
import UIKit_iOS
import FoundationExt

final class TransactionSuccessViewController: UIViewController, ViewHolder {
    typealias RootViewType = TransactionSuccessViewLayout

    let presenter: TransactionSuccessPresenterProtocol

    init(
        presenter: TransactionSuccessPresenterProtocol,
    ) {
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = TransactionSuccessViewLayout()
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        setupHandlers()
        setupLocalization()

        presenter.setup()
    }

    private func setupHandlers() {
        rootView.doneButton.addTarget(
            self,
            action: #selector(actionDone),
            for: .touchUpInside
        )
    }

    private func setupLocalization() {
        rootView.titleLabel.text = String(localized: .transactionSuccessTitle)
        rootView.doneButton.setTitle(String(localized: .Common.done))
    }

    @objc func actionDone() {
        presenter.activateDone()
    }
}

extension TransactionSuccessViewController: TransactionSuccessViewProtocol {}
