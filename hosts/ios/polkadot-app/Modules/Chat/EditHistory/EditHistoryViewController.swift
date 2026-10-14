import UIKit
import UIKit_iOS
import PolkadotUI
import FoundationExt

final class EditHistoryViewController: UIViewController, ViewHolder {
    typealias RootViewType = EditHistoryViewLayout

    var presenter: EditHistoryPresenterProtocol?

    private let timestampFormatter: TimestampFormatting

    init(timestampFormatter: TimestampFormatting) {
        self.timestampFormatter = timestampFormatter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = EditHistoryViewLayout()
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        presenter?.setup()
    }
}

// MARK: - EditHistoryViewProtocol

extension EditHistoryViewController: EditHistoryViewProtocol {
    func didReceive(viewModel: EditHistoryViewModel) {
        let layoutViewModel = EditHistoryViewLayout.ViewModel(
            currentMessage: .init(
                text: viewModel.currentMessage.text,
                formattedTimestamp: timestampFormatter.string(for: viewModel.currentMessage.timestamp),
                statusImage: viewModel.currentMessage.outboxStatus?.image
            ),
            historyItems: viewModel.historyItems.map { item in
                .init(
                    diffParts: item.diffParts,
                    formattedTimestamp: timestampFormatter.string(for: item.timestamp)
                )
            },
            isOutgoing: viewModel.isOutgoing
        )
        rootView.bind(viewModel: layoutViewModel)
    }
}
