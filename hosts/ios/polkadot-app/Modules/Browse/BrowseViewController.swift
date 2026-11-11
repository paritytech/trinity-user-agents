import FoundationExt
import UIKit

final class BrowseViewController: UIViewController, ViewHolder {
    typealias RootViewType = BrowseViewLayout

    let presenter: BrowsePresenterProtocol

    init(presenter: BrowsePresenterProtocol) {
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = BrowseViewLayout()
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        presenter.setup()
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)

        presenter.didAppear()
    }

    override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated)

        presenter.didDisappear()
    }
}

// MARK: - BrowseViewProtocol

extension BrowseViewController: BrowseViewProtocol {
    func showLoading() {
        rootView.activityIndicatorView.startAnimating()
    }

    func hideLoading() {
        rootView.activityIndicatorView.stopAnimating()
    }
}
