import UIKit
import FoundationExt

final class LocalAuthViewController: UIViewController, ViewHolder {
    typealias RootViewType = LocalAuthViewLayout

    let presenter: LocalAuthPresenterProtocol

    init(presenter: LocalAuthPresenterProtocol) {
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = LocalAuthViewLayout()
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        rootView.actionButton.addTarget(self, action: #selector(actionRetry), for: .touchUpInside)

        presenter.setup()
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)

        if rootView.actionView.isLoading {
            rootView.actionView.updateAnimation()
        }
    }

    @objc func actionRetry() {
        presenter.retryAuth()
    }
}

extension LocalAuthViewController: LocalAuthViewProtocol {
    func didStartAuth() {
        rootView.actionView.startLoading()
    }

    func didStopAuth() {
        rootView.actionView.stopLoading()
    }
}
