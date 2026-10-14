import UIKit
import SwiftUI
import PolkadotUI

final class ChatRequestListViewController: UIHostingController<ChatRequestListViewLayout> {
    let presenter: ChatRequestListPresenterProtocol
    private let viewModel = ChatRequestListViewModel()
    private let dateFormatter = ContactTimestampFormatter()

    init(presenter: ChatRequestListPresenterProtocol) {
        self.presenter = presenter
        super.init(rootView: ChatRequestListViewLayout(viewModel: viewModel, dateFormatter: dateFormatter))

        viewModel.onItemSelection = { [weak self] id in
            self?.presenter.selectRequest(with: id)
        }
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .bgSurfaceMain

        title = String(localized: .chatRequestsListTitle)

        presenter.setup()
    }
}

extension ChatRequestListViewController: ChatRequestListViewProtocol {
    func didReceive(items: [ChatRequestListItem]) {
        viewModel.items = items
    }
}
