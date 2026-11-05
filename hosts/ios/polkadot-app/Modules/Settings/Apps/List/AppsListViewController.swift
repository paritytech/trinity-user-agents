import struct TrUAPIHost.ProductId
import UIKit
import SwiftUI
import PolkadotUI

final class AppsListViewController: UIHostingController<AppsListViewLayout<ProductId>> {
    let presenter: AppsListPresenterProtocol

    init(presenter: AppsListPresenterProtocol) {
        self.presenter = presenter
        super.init(rootView: AppsListViewLayout<ProductId>())
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
    func didReceive(items: [AppsListViewLayout<ProductId>.Item]) {
        rootView.viewModel.items = items
    }
}
