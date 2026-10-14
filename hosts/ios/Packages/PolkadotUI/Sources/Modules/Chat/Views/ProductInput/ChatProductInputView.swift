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
    }

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
        stack.addArrangedSubview(activityLine)
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

    func makeCustomFooter(_ footer: ProductWidgetChatView) -> UIView {
        let content = SwiftUIContentConfiguration(
            view: footer,
            margins: EdgeInsets(top: 0, leading: 16, bottom: 0, trailing: 16)
        ).makeContentView()
        let scrollView = UIScrollView()
        scrollView.addSubview(content)
        content.snp.makeConstraints {
            $0.edges.equalTo(scrollView.contentLayoutGuide)
            $0.width.equalTo(scrollView.frameLayoutGuide)
        }

        let maxHeight = (window?.windowScene?.screen.bounds.height ?? 800) * Self.customFooterHeightRatio
        scrollView.snp.makeConstraints {
            $0.height.equalTo(content).priority(.high)
            $0.height.lessThanOrEqualTo(maxHeight)
        }
        return scrollView
    }

    func refresh(notify: Bool) {
        let model = configuration.accessory
        let text = typedText

        activityLine.text = model.activityText
        suggestionsStrip.bind(model.suggestions)
        suggestionsStrip.isHidden = model.suggestions.isEmpty || !text.isEmpty
        catalogue.bind(model.matchingCommands(for: text))
        catalogue.superview?.isHidden = catalogue.isHidden

        if notify {
            handler?.chatInputDidChange()
        }
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
    }
}

// MARK: - Presenting protocols

extension ChatProductInputView: ChatTextInputPresenting {
    func activateTextField() {
        input?.activateTextField()
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
