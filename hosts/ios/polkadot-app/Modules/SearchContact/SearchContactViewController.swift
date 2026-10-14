import UIKit
import PolkadotUI
import FoundationExt

final class SearchContactViewController: UIViewController, ViewHolder {
    typealias RootViewType = SearchContactViewLayout

    let presenter: SearchContactPresenterProtocol

    init(presenter: SearchContactPresenterProtocol) {
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = SearchContactViewLayout()
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        addHandlers()
        presenter.setup()
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        rootView.focusSearchInput()
    }
}

private extension SearchContactViewController {
    func addHandlers() {
        rootView.searchHandler = { [weak self] text in
            self?.presenter.search(username: text ?? "")
        }

        rootView.cancelHandler = { [weak self] in
            self?.dismiss(animated: true)
        }

        rootView.selectionHandler = { [weak self] identifier in
            self?.presenter.didSelectContact(identifier: identifier)
        }
    }
}

extension SearchContactViewController: SearchContactViewProtocol {
    func didReceive(viewModel: SearchContactResultsView.ViewModel) {
        rootView.bind(viewModel: viewModel)
    }

    func didReceive(status: SearchContactResultsView.StatusViewModel) {
        rootView.bind(status: status)
    }
}
