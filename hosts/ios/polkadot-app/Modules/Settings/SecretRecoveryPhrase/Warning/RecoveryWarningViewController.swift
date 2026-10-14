import UIKit
import FoundationExt

final class RecoveryWarningViewController: UIViewController, ViewHolder {
    typealias RootViewType = RecoveryWarningViewLayout

    let presenter: RecoveryWarningPresenterProtocol

    init(
        presenter: RecoveryWarningPresenterProtocol
    ) {
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = RecoveryWarningViewLayout()
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        setupHandlers()
        setup()

        presenter.setup()
    }
}

private extension RecoveryWarningViewController {
    func setup() {
        rootView.closeButton.setTitle(
            String(localized: .Common.cancel)
        )

        rootView.actionButton.setTitle(
            String(localized: .secretWarningActionShow)
        )
    }

    func setupHandlers() {
        let closeAction = UIAction { [weak presenter] _ in
            presenter?.onClose()
        }
        rootView.closeButton.addAction(closeAction, for: .touchUpInside)

        let action = UIAction { [weak presenter] _ in
            presenter?.onAction()
        }
        rootView.actionButton.addAction(action, for: .touchUpInside)
    }
}

extension RecoveryWarningViewController: RecoveryWarningViewProtocol {
    func didReceive(viewModels: [RecoveryWarningViewLayout.Model]) {
        rootView.layoutContent.steps = viewModels
    }
}
