import UIKit
import PolkadotUI
import Combine
import SwiftUI
import Foundation_iOS
import FoundationExt

class ChatViewController: UIViewController, ViewHolder {
    typealias RootViewType = ChatViewLayout

    var readTimer: Timer?
    var lastVisibleItem: ChatViewLayout.ItemIdentifierType?

    let presenter: ChatPresenterProtocol
    var timerCancellable: AnyCancellable?

    var keyboardHandler: KeyboardHandler?

    lazy var navigationBarController: ChatNavigationBarControlling = ChatNavigationBarControllerFactory.make(
        navigationItem: navigationItem,
        titleView: rootView.centerItem,
        onStartCall: { [weak presenter] in presenter?.startCall($0) }
    )

    private var shouldAutoscrollOnKeyboardChange: Bool = true

    private var inputAutoFocus: InputAutoFocus = .awaitingAppearance

    private lazy var reactionTimestampFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.dateFormat = "h:mm a"
        return formatter
    }()

    init(
        presenter: ChatPresenterProtocol
    ) {
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = ChatViewLayout()
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        presenter.viewWillAppear()
        startUpdateTimer()

        guard keyboardHandler == nil else {
            return
        }
        setupKeyboardHandler()
        keyboardHandler?.delegate = self
    }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        presenter.viewWillDisappear()
        stopUpdateTimer()
        cancelReadTimer()

        clearKeyboardHandler()
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        startReadTimer()

        guard inputAutoFocus != .done else { return }

        inputAutoFocus = .awaitingFocus

        resolveInputAutoFocus()
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        navigationBarController.configure()
        presenter.setup()
        addHandlers()
    }

    private func startUpdateTimer() {
        timerCancellable = Timer.publish(
            every: AppConfig.timestampRefreshInterval,
            on: RunLoop.main,
            in: .default
        )
        .autoconnect()
        .prepend(.now)
        .sink { [rootView] _ in
            rootView.updateCells()
        }
    }

    private func stopUpdateTimer() {
        timerCancellable?.cancel()
    }
}

extension ChatViewController: ChatViewProtocol {
    func didReceive(viewModel: ChatViewLayout.ViewModel) {
        rootView.bind(viewModel: viewModel)
        navigationBarController.update(headerConfiguration: rootView.contactHeaderConfiguration)

        resolveInputAutoFocus()
    }

    func showReply(messageId: String, username: String, text: String) {
        rootView.activateReply(messageId: messageId, username: username, text: text)
    }

    func showEdit(messageId: String, currentText: String) {
        rootView.activateEdit(messageId: messageId, currentText: currentText)
    }

    func showReactionDetails(viewModel: ReactionDetailsViewModel) {
        let wrappedView = ReactionDetailsPopupView(
            viewModel: viewModel,
            timestampFormatter: reactionTimestampFormatter,
            onDismiss: { [weak self] in
                self?.dismiss(animated: true)
            }
        )

        let hostingController = UIHostingController(rootView: wrappedView)
        hostingController.view.backgroundColor = .clear
        hostingController.modalPresentationStyle = .overFullScreen
        hostingController.modalTransitionStyle = .crossDissolve

        present(hostingController, animated: true)
    }

    func didReceive(footer: (any HashableContentConfiguration)?) {
        rootView.setFooter(footer)
    }

    func didReceive(callActions: [ChatCallType]) {
        navigationBarController.apply(callActions: callActions)
    }

    func didReceive(contactMenu: UIMenu?) {
        navigationBarController.apply(contactMenu: contactMenu)
    }
}

private extension ChatViewController {
    func resolveInputAutoFocus() {
        guard inputAutoFocus == .awaitingFocus, rootView.activatesInputOnAppear else {
            return
        }

        inputAutoFocus = .done
        rootView.focusInput()
    }

    func addHandlers() {
        rootView.onSendTap = { [weak self] text, replyToMessageId in
            self?.presenter.send(text: text, replyToMessageId: replyToMessageId)
        }
        rootView.onEditSendTap = { [weak self] messageId, newText in
            self?.presenter.sendEdit(messageId: messageId, newText: newText)
        }
        rootView.onTransferTap = { [weak self] in
            self?.presenter.makeTransfer()
        }
        rootView.onAttachmentTap = { [weak self] in
            self?.presenter.showAttachmentSelection()
        }

        rootView.onReplyMessage = { [weak self] messageId in
            self?.presenter.onReply(for: messageId)
        }

        rootView.onScrollToBottomTap = { [weak presenter] in
            presenter?.onScrollToBottom()
        }

        rootView.onScrollToReactionTap = { [weak presenter] in
            presenter?.onScrollToReaction()
        }
    }
}

extension ChatViewController: KeyboardAdoptable {
    func updateWhileKeyboardFrameChanging(_: CGRect) {}
}

extension ChatViewController: KeyboardHandlerDelegate {
    func keyboardWillShow(notification: Notification) {
        handle(notification)
    }

    func keyboardWillHide(notification _: Notification) {
        rootView.adoptToHiddenKeyboard()
    }

    private func handle(_ notification: Notification) {
        guard
            let userInfo = notification.userInfo,
            let endFrame = userInfo[UIResponder.keyboardFrameEndUserInfoKey] as? CGRect
        else {
            return
        }

        let keyboardHeight = endFrame.height - view.safeAreaInsets.bottom
        rootView.adoptToVisibleKeyboard(bottomInset: keyboardHeight)
    }
}

private extension ChatViewController {
    func startReadTimer() {
        readTimer?.invalidate()
        let readTimer = Timer(timeInterval: 0.1, repeats: true) { [weak self] timer in
            guard let self else {
                timer.invalidate()
                return
            }
            readTimerDidFire()
        }
        self.readTimer = readTimer
        RunLoop.main.add(readTimer, forMode: .common)
    }

    func cancelReadTimer() {
        readTimer?.invalidate()
        readTimer = nil
    }

    private func readTimerDidFire() {
        markVisibleMessagesAsRead()
    }

    private func markVisibleMessagesAsRead() {
        let visibleItems = rootView.visibleIdentifiers()
        guard
            let last = visibleItems.last,
            lastVisibleItem != last
        else {
            return
        }
        presenter.readTillMessage(identifier: last)
        lastVisibleItem = last
    }
}

private extension ChatViewController {
    enum InputAutoFocus {
        case awaitingAppearance
        case awaitingFocus
        case done
    }
}
