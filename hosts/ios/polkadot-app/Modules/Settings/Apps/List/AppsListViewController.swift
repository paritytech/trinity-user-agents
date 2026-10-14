import UIKit
import SwiftUI
import PolkadotUI

final class AppsListViewController: UIHostingController<AppsListViewLayout> {
    let presenter: AppsListPresenterProtocol

    init(presenter: AppsListPresenterProtocol) {
        self.presenter = presenter
        super.init(rootView: AppsListViewLayout())
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        view.backgroundColor = .bgSurfaceMain
        title = String(localized: .appsListTitle)

        rootView.viewModel.onSelect = { [weak self] item in
            self?.presenter.selectApp(item)
        }

        presenter.setup()
    }
}

extension AppsListViewController: AppsListViewProtocol {
    func didReceive(items: [AppsListViewLayout.Item]) {
        rootView.viewModel.items = items
    }
}
