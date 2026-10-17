import DesignSystem
import SwiftUI
import UIKit
internal import SnapKit

/// The footer of a product room. It forwards the input events to the room screen and keeps the quick replies, the
/// slash command catalogue and the activity line in step with the text and with the accessory model.
final class ChatProductInputView: UIView {
    /// The custom footer is at most this part of the height of the screen (P6.6).
    static let customFooterHeightRatio: CGFloat = 0.4

    private let configuration: ChatProductInputConfiguration
    private weak var handler: ChatInputHandling?

    private let stack: UIStackView = create {
        $0.axis = .vertical
        $0.spacing = DSSpacings.extraSmall
    }

    private let catalogue = ChatCommandCatalogueView()
    private let activityLine = ChatActivityLine()
    private let readAloudButton: UIButton = {
        var configuration = UIButton.Configuration.plain()
        configuration.image = UIImage(systemName: "stop.circle")
        configuration.imagePadding = 6
        configuration.baseForegroundColor = .fgPrimary
        let button = UIButton(configuration: configuration)
        button.contentHorizontalAlignment = .leading
        button.isHidden = true
        return button
    }()

    private let suggestionsStrip = ChatSuggestionsStrip()
    private var input: DSChatInputView?
    private var observation: ChatRoomAccessoryObservation?

    init(configuration: ChatProductInputConfiguration, handler: ChatInputHandling?) {
        self.configuration = configuration
        self.handler = handler
        super.init(frame: .zero)

        setupViews()
        observation = configuration.accessory.observe { [weak self] in
            self?.refresh(notify: true)
        }
        refresh(notify: false)
        observeE2EText()
    }

    deinit {
        textObserver.map(NotificationCenter.default.removeObserver)
    }

    private var lastHeight: CGFloat = 0

    /// The tree of a custom footer and the activity line change the height of the footer after it binds, so the
    /// room screen measures it again.
    override func layoutSubviews() {
        super.layoutSubviews()

        guard abs(bounds.height - lastHeight) > 0.5 else { return }
        lastHeight = bounds.height
        DispatchQueue.main.async { [weak self] in
            self?.handler?.chatInputDidChange()
        }
    }

    private var textObserver: NSObjectProtocol?

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    /// The text that the user typed.
    var typedText: String {
        input?.textView.text ?? ""
    }
}

private extension ChatProductInputView {
    func setupViews() {
        backgroundColor = .clear

        addSubview(stack)
        stack.snp.makeConstraints { $0.edges.equalToSuperview() }

        let catalogueContainer = UIView()
        catalogueContainer.addSubview(catalogue)
        catalogue.snp.makeConstraints { $0.edges.equalToSuperview().inset(UIEdgeInsets(
            top: 0,
            left: 8,
            bottom: 0,
            right: 8
        )) }

        stack.addArrangedSubview(catalogueContainer)
        stack.addArrangedSubview(readAloudButton)
        stack.addArrangedSubview(activityLine)

        readAloudButton.addAction(UIAction { [weak self] _ in
            self?.configuration.accessory.onStopReadAloud?()
        }, for: .touchUpInside)
        stack.addArrangedSubview(suggestionsStrip)

        suggestionsStrip.onSelect = { [weak self] suggestion in
            self?.configuration.accessory.onSuggestion?(suggestion)
        }
        catalogue.onPick = { [weak self] command in
            self?.complete(command)
        }

        switch configuration.body {
        case let .input(inputConfiguration, _):
            let input = DSChatInputView(configuration: inputConfiguration, handler: self)
            self.input = input
            stack.addArrangedSubview(input)
        case .empty:
            break
        case let .custom(footer):
            stack.addArrangedSubview(makeCustomFooter(footer))
        }
    }

    /// The tree of the product on the background of the screen, at most 40 % of the screen high. A taller tree
    /// is cut at the bottom.
    func makeCustomFooter(_ footer: ProductWidgetChatView) -> UIView {
        let content = SwiftUIContentConfiguration(
            view: footer,
            margins: EdgeInsets(top: 8, leading: 16, bottom: 8, trailing: 16)
        ).makeContentView()

        let container = UIView()
        container.backgroundColor = .bgSurfaceMain
        container.clipsToBounds = true
        container.addSubview(content)
        content.snp.makeConstraints {
            $0.top.leading.trailing.equalToSuperview()
            $0.bottom.equalToSuperview().priority(.high)
        }

        let maxHeight = UIScreen.main.bounds.height * Self.customFooterHeightRatio
        container.snp.makeConstraints { $0.height.lessThanOrEqualTo(maxHeight) }
        return container
    }

    func refresh(notify: Bool) {
        let model = configuration.accessory
        let text = typedText

        activityLine.text = model.activityText
        activityLine.icon = model.activityIcon
        readAloudButton.configuration?.title = model.readAloudStopTitle
        readAloudButton.isHidden = model.readAloudStopTitle == nil
        suggestionsStrip.bind(model.suggestions)
        suggestionsStrip.isHidden = model.suggestions.isEmpty || (!text.isEmpty && !model.followsInput)
        catalogue.bind(model.matchingCommands(for: text))
        catalogue.superview?.isHidden = catalogue.isHidden

        if notify {
            handler?.chatInputDidChange()
        }
    }

    /// The truapi E2E harness types into the input of a simulator build, as the user does.
    func observeE2EText() {
        #if targetEnvironment(simulator)
            textObserver = NotificationCenter.default.addObserver(
                forName: Notification.Name("TrUAPIE2ESetInputText"),
                object: nil,
                queue: .main
            ) { [weak self] notification in
                MainActor.assumeIsolated {
                    guard let self, let input = self.input, let text = notification.object as? String else { return }
                    input.textView.text = text
                    input.textViewDidChange(input.textView)
                }
            }
        #endif
    }

    func complete(_ command: ChatRoomAccessoryModel.Command) {
        guard let input else { return }
        input.textView.text = "/" + command.name + " "
        input.textViewDidChange(input.textView)
        input.activateTextField()
    }
}

// MARK: - ChatInputHandling

extension ChatProductInputView: ChatInputHandling {
    func chatInputDidSend(_ text: String) {
        handler?.chatInputDidSend(text)
        refresh(notify: true)
        if configuration.accessory.followsInput {
            configuration.accessory.onInputText?("")
        }
    }

    func chatInputDidTransfer() {
        handler?.chatInputDidTransfer()
    }

    func chatInputDidAttachment() {
        if case let .input(_, onAttach?) = configuration.body {
            onAttach()
        } else {
            handler?.chatInputDidAttachment()
        }
    }

    func chatInputDidCancelReply() {
        handler?.chatInputDidCancelReply()
    }

    func chatInputDidCancelEdit() {
        handler?.chatInputDidCancelEdit()
    }

    func chatInputDidChange() {
        refresh(notify: false)
        handler?.chatInputDidChange()
        if configuration.accessory.followsInput {
            configuration.accessory.onInputText?(typedText)
        }
    }
}

// MARK: - Presenting protocols

extension ChatProductInputView: ChatTextInputPresenting {
    func activateTextField() {
        input?.activateTextField()
    }

    func restoreText(_ text: String) {
        input?.restoreText(text)
    }
}

extension ChatProductInputView: ChatInputReplyPresenting {
    func showReplyBanner(title: String, messageText: String) {
        input?.showReplyBanner(title: title, messageText: messageText)
    }

    func hideReplyBanner() {
        input?.hideReplyBanner()
    }
}

extension ChatProductInputView: ChatInputEditPresenting {
    var isEditing: Bool {
        input?.isEditing ?? false
    }

    func showEditBanner(title: String, currentText: String) {
        input?.showEditBanner(title: title, currentText: currentText)
    }

    func hideEditBanner() {
        input?.hideEditBanner()
    }
}
