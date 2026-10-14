import Foundation
import UIKit
import DesignSystem
import ExternalAccessibility

public final class ChatRequestInProgressBannerView: UIView {
    let header: GenericPairValueView<Label, Label> = .create { view in
        view.fView.typography = .bodyMediumEmphasized
        view.fView.textColor = UIColor.fgSecondary
        view.fView.textAlignment = .center

        view.sView.typography = .bodyMedium
        view.sView.textColor = UIColor.fgTertiary
        view.sView.textAlignment = .center
        view.sView.numberOfLines = 0

        view.makeVertical()
        view.spacing = 4
    }

    let chatInputView: DSChatInputView

    public init(viewModel: ViewModel) {
        chatInputView = DSChatInputView(configuration: viewModel.inputConfig, handler: nil)

        super.init(frame: .zero)

        setupLayout()
        configureStyle()
        setupHeader(with: viewModel.username)
        applyAccessibilityBindings()
    }

    @available(*, unavailable)
    @MainActor required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

private extension ChatRequestInProgressBannerView {
    func setupLayout() {
        addSubview(chatInputView)
        addSubview(header)

        chatInputView.snp.makeConstraints { make in
            make.leading.trailing.bottom.equalToSuperview()
        }

        header.snp.makeConstraints { make in
            make.leading.trailing.equalToSuperview().inset(16)
            make.top.equalToSuperview().inset(8)
            make.bottom.equalTo(chatInputView.snp.top).offset(-8)
        }
    }

    func configureStyle() {
        backgroundColor = UIColor.bgSurfaceMain
    }

    func setupHeader(with username: String) {
        header.fView.text = String(localized: .chatRequestInProgressTitle(username: username))
        header.sView.text = String(localized: .chatRequestInProgressMessage)
    }
}

extension ChatRequestInProgressBannerView: ChatTextInputPresenting {
    public func activateTextField() {
        chatInputView.activateTextField()
    }
}

public extension ChatRequestInProgressBannerView {
    struct ViewModel: Equatable {
        let username: String
        let inputConfig: ChatInputViewConfiguration

        public init(username: String, inputConfig: ChatInputViewConfiguration) {
            self.username = username
            self.inputConfig = inputConfig
        }
    }
}

extension ChatRequestInProgressBannerView.ViewModel: ChatInputViewConfigurationProtocol {
    public var activateOnAppear: Bool {
        true
    }

    public func makeContentView(for handler: ChatInputHandling?) -> UIView {
        let view = ChatRequestInProgressBannerView(viewModel: self)
        view.chatInputView.inputHandler = handler
        return view
    }

    public func equalsTo(configuration: any ChatInputViewConfigurationProtocol) -> Bool {
        guard let otherConfig = configuration as? ChatRequestInProgressBannerView.ViewModel else {
            return false
        }

        return self == otherConfig
    }
}

#Preview(traits: .fixedLayout(width: 375, height: 300)) {
    ChatRequestInProgressBannerView.ViewModel(
        username: "Marcelos.87",
        inputConfig: .chatRequest()
    )
    .makeContentView(for: nil)
}

// MARK: - AccessibilityBound

extension ChatRequestInProgressBannerView: AccessibilityBound {
    public var accessibilityBindings: [AccessibilityBinding] {
        [.init(header, AccessibilityID.Chats.inviteHeader)]
    }
}
