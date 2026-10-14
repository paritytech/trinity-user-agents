import UIKit
import FoundationExt

final class LeaveChatViewController: UIViewController, ViewHolder {
    typealias RootViewType = LeaveChatViewLayout

    private let username: String
    private let onDelete: () -> Void
    private let onCancel: () -> Void

    init(
        username: String,
        onDelete: @escaping () -> Void,
        onCancel: @escaping () -> Void
    ) {
        self.username = username
        self.onDelete = onDelete
        self.onCancel = onCancel
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = LeaveChatViewLayout()
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        rootView.bind(username: username)
        setupActions()
    }
}

// MARK: - Private functions

extension LeaveChatViewController {
    private func setupActions() {
        rootView.deleteButton.addTarget(self, action: #selector(handleDelete), for: .touchUpInside)
        rootView.cancelButton.addTarget(self, action: #selector(handleCancel), for: .touchUpInside)
    }

    @objc
    private func handleDelete() {
        dismiss(animated: true) { [onDelete] in
            onDelete()
        }
    }

    @objc
    private func handleCancel() {
        dismiss(animated: true) { [onCancel] in
            onCancel()
        }
    }
}
