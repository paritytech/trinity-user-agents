import UIKit
import FoundationExt

final class CancelChatRequestViewController: UIViewController, ViewHolder {
    typealias RootViewType = CancelChatRequestViewLayout

    private let username: String
    private let onConfirm: () -> Void
    private let onKeep: () -> Void

    init(
        username: String,
        onConfirm: @escaping () -> Void,
        onKeep: @escaping () -> Void
    ) {
        self.username = username
        self.onConfirm = onConfirm
        self.onKeep = onKeep
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = CancelChatRequestViewLayout()
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        rootView.bind(username: username)
        setupActions()
    }
}

// MARK: - Private functions

extension CancelChatRequestViewController {
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
        dismiss(animated: true) { [onKeep] in
            onKeep()
        }
    }
}
