import UIKit
import SwiftUI
import PolkadotUI

final class BlockedUsersViewController: UIHostingController<BlockedUsersViewLayout> {
    let presenter: BlockedUsersPresenterProtocol

    init(presenter: BlockedUsersPresenterProtocol) {
        self.presenter = presenter
        super.init(rootView: BlockedUsersViewLayout())
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        view.backgroundColor = .bgSurfaceMain
        title = String(localized: .blockedUsersTitle)

        rootView.viewModel.onSelect = { [weak self] item in
            self?.presenter.selectUser(item)
        }
        rootView.viewModel.onUnblock = { [weak self] item in
            self?.presenter.unblockUser(item)
        }

        presenter.setup()
    }
}

extension BlockedUsersViewController: BlockedUsersViewProtocol {
    func didReceive(items: [BlockedUsersViewLayout.Item]) {
        rootView.viewModel.items = items
    }
}
