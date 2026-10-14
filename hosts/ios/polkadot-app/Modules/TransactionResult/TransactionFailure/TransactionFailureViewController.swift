import UIKit
import FoundationExt

final class TransactionFailureViewController: UIViewController, ViewHolder {
    typealias RootViewType = TransactionFailureViewLayout

    let presenter: TransactionFailurePresenterProtocol

    init(presenter: TransactionFailurePresenterProtocol) {
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = TransactionFailureViewLayout()
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        setupHandlers()
        setupLocalization()

        presenter.setup()
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)

        presenter.onAppear()
    }

    func setupHandlers() {
        rootView.actionButton.addTarget(
            self,
            action: #selector(actionRetry),
            for: .touchUpInside
        )
    }

    func setupLocalization() {
        rootView.titleLabel.text = String(localized: .transactionFailureTitle)
        rootView.detailsLabel.text = String(localized: .transactionFailureDetails)

        rootView.actionButton.setTitle(String(localized: .Common.retry))
    }

    @objc func actionRetry() {
        presenter.onAction()
    }
}

extension TransactionFailureViewController: TransactionFailureViewProtocol {}
