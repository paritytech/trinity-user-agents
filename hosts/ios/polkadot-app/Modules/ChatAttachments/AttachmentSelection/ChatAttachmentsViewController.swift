import UIKit
import PolkadotUI
import Foundation_iOS
import FoundationExt

final class ChatAttachmentsViewController: UIViewController, ViewHolder {
    typealias RootViewType = ChatAttachmentsViewLayout

    let presenter: ChatAttachmentsPresenterProtocol

    var keyboardHandler: KeyboardHandler?

    init(presenter: ChatAttachmentsPresenterProtocol) {
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = ChatAttachmentsViewLayout()
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        configureNavigationBar()
        configureLayout()

        rootView.startLoading()

        presenter.setup()
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)

        guard keyboardHandler == nil else { return }
        setupKeyboardHandler()
    }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        clearKeyboardHandler()
    }
}

private extension ChatAttachmentsViewController {
    func configureLayout() {
        rootView.backgroundColor = .bgSurfaceMain
        rootView.parentViewController = self

        rootView.onSendTap = { [weak self] text in
            self?.presenter.send(with: text)
        }
    }

    func configureNavigationBar() {
        let closeItem = UIBarButtonItem(
            image: .buttonClose.withRenderingMode(.alwaysTemplate),
            style: .plain,
            target: self,
            action: #selector(actionClose)
        )

        navigationItem.leftBarButtonItem = closeItem
    }

    @objc func actionClose() {
        presenter.cancel()
    }
}

extension ChatAttachmentsViewController: ChatAttachmentsViewProtocol {
    func didReceive(viewModels: [AttachmentSelectionViewModel]) {
        rootView.setAttachments(viewModels)
        rootView.stopLoading()
    }
}

extension ChatAttachmentsViewController: KeyboardAdoptable {}
