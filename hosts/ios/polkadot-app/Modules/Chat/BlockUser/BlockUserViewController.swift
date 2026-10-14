import UIKit
import FoundationExt
import PolkadotUI

final class BlockUserViewController: UIViewController, ViewHolder {
    typealias RootViewType = BlockUserViewLayout

    private let username: String
    private let onBlock: () -> Void
    private let onCancel: () -> Void

    init(
        username: String,
        onBlock: @escaping () -> Void,
        onCancel: @escaping () -> Void
    ) {
        self.username = username
        self.onBlock = onBlock
        self.onCancel = onCancel
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = BlockUserViewLayout()
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        rootView.bind(username: username)
        setupActions()
    }
}

// MARK: - Private functions

extension BlockUserViewController {
    private func setupActions() {
        rootView.blockButton.addTarget(self, action: #selector(handleBlock), for: .touchUpInside)
        rootView.cancelButton.addTarget(self, action: #selector(handleCancel), for: .touchUpInside)
    }

    @objc
    private func handleBlock() {
        dismiss(animated: true) { [onBlock] in
            onBlock()
        }
    }

    @objc
    private func handleCancel() {
        dismiss(animated: true) { [onCancel] in
            onCancel()
        }
    }
}
