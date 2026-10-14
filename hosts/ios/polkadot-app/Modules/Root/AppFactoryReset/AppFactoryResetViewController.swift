#if TESTNET_FEATURE
    import UIKit
    import UIKit_iOS
    import FoundationExt

    final class AppFactoryResetViewController: UIViewController, ViewHolder {
        typealias RootViewType = AppFactoryResetViewLayout

        let presenter: AppFactoryResetPresenterProtocol

        var allowsSwipeDown: Bool = false

        init(presenter: AppFactoryResetPresenterProtocol) {
            self.presenter = presenter
            super.init(nibName: nil, bundle: nil)
        }

        @available(*, unavailable)
        required init?(coder _: NSCoder) {
            fatalError("init(coder:) has not been implemented")
        }

        override func loadView() {
            view = AppFactoryResetViewLayout()
        }

        override func viewDidLoad() {
            super.viewDidLoad()
            setupHandlers()
        }

        private func setupHandlers() {
            rootView.startOverButton.addTarget(
                self,
                action: #selector(handleStartOver),
                for: .touchUpInside
            )
            rootView.dismissButton.addTarget(
                self,
                action: #selector(handleDismiss),
                for: .touchUpInside
            )
        }

        @objc private func handleStartOver() {
            presenter.actionStartOver()
        }

        @objc private func handleDismiss() {
            presenter.actionDismiss()
        }
    }

    extension AppFactoryResetViewController: AppFactoryResetViewProtocol {}

    extension AppFactoryResetViewController: ModalPresenterDelegate {}
#endif
