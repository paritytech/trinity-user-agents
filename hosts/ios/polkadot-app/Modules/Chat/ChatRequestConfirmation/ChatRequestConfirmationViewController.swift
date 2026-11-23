import UIKit
import FoundationExt

final class ChatRequestConfirmationViewController: UIViewController, ViewHolder {
    typealias RootViewType = ChatRequestConfirmationViewLayout

    private let viewModel: ChatRequestConfirmationViewModel
    private let onConfirm: () -> Void

    init(
        viewModel: ChatRequestConfirmationViewModel,
        onConfirm: @escaping () -> Void
    ) {
        self.viewModel = viewModel
        self.onConfirm = onConfirm
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = ChatRequestConfirmationViewLayout()
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        rootView.bind(viewModel: viewModel)
        setupActions()
    }
}

// MARK: - Private functions

extension ChatRequestConfirmationViewController {
    private func setupActions() {
        rootView.confirmButton.addTarget(self, action: #selector(handleConfirm), for: .touchUpInside)
        rootView.keepButton.addTarget(self, action: #selector(handleKeep), for: .touchUpInside)
    }

    @objc
    private func handleConfirm() {
        dismiss(animated: true) { [onConfirm] in
            onConfirm()
        }
    }

    @objc
    private func handleKeep() {
        dismiss(animated: true)
    }
}
