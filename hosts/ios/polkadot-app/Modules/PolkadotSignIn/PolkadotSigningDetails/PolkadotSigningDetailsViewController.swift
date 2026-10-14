import UIKit
import PolkadotUI
import FoundationExt

final class PolkadotSigningDetailsViewController: UIViewController, ViewHolder {
    typealias RootViewType = PolkadotSigningDetailsViewLayout

    let presenter: PolkadotSigningDetailsPresenterProtocol

    init(presenter: PolkadotSigningDetailsPresenterProtocol) {
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = PolkadotSigningDetailsViewLayout()
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        presenter.setup()
    }
}

extension PolkadotSigningDetailsViewController: PolkadotSigningDetailsViewProtocol {
    func didReceive(viewModel: PolkadotSigningDetailsViewLayout.ViewModel) {
        navigationItem.title = viewModel.isTransaction
            ? .init(localized: .transactionDetailsTitle)
            : .init(localized: .messageDetailsTitle)

        rootView.bind(viewModel: viewModel)
    }
}
