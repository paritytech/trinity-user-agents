import SwiftUI
import UIKit
internal import SnapKit

// MARK: - Configuration

public struct ChatRichTextMessageConfiguration: HashableContentConfiguration {
    public static let reuseIdentifier = "ChatRichTextMessageView"

    public let attachmentItems: [AttachmentItem]
    public let textViewModel: ChatMessageTextView.ViewModel?
    public let productLinkPreview: ChatProductLinkPreviewConfiguration?

    public init(
        attachmentItems: [AttachmentItem] = [],
        textViewModel: ChatMessageTextView.ViewModel? = nil,
        productLinkPreview: ChatProductLinkPreviewConfiguration? = nil
    ) {
        self.attachmentItems = attachmentItems
        self.textViewModel = textViewModel
        self.productLinkPreview = productLinkPreview
    }

    public func makeContentView() -> any UIView & UIContentView {
        ChatRichTextMessageView(configuration: self)
    }
}

// MARK: - Model types

public extension ChatRichTextMessageConfiguration {
    struct AttachmentItem: Hashable {
        public let identifier: String
        public let mediaConfiguration: ChatMessageMediaViewConfiguration

        public init(identifier: String, mediaConfiguration: ChatMessageMediaViewConfiguration) {
            self.identifier = identifier
            self.mediaConfiguration = mediaConfiguration
        }

        public static func == (lhs: Self, rhs: Self) -> Bool {
            lhs.identifier == rhs.identifier
        }

        public func hash(into hasher: inout Hasher) {
            hasher.combine(identifier)
        }
    }
}

// MARK: - Content View

final class ChatRichTextMessageView: UIView, UIContentView {
    private enum Layout {
        static let stackSpacing: CGFloat = 8
        static let textLeadingInsetWithAttachments: CGFloat = 8
        static let textTrailingInsetWithAttachments: CGFloat = 2
    }

    private let stackView: UIStackView = .create {
        $0.axis = .vertical
        $0.alignment = .fill
        $0.spacing = Layout.stackSpacing
    }

    private var mediaView: MediaAttachmentView?
    private var textHostingView: (any UIContentView & UIView)?
    private var productLinkPreviewView: (any UIContentView & UIView)?

    private var appliedConfiguration: ChatRichTextMessageConfiguration

    var configuration: UIContentConfiguration {
        get { appliedConfiguration }
        set { apply(newValue) }
    }

    override var intrinsicContentSize: CGSize {
        if stackView.arrangedSubviews.isEmpty {
            return .zero
        }
        return super.intrinsicContentSize
    }

    init(configuration: ChatRichTextMessageConfiguration) {
        appliedConfiguration = configuration
        super.init(frame: .zero)

        addSubview(stackView)
        stackView.snp.makeConstraints { make in
            make.directionalEdges.equalToSuperview()
        }

        apply(configuration)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    // MARK: - Apply

    private func apply(_ any: UIContentConfiguration) {
        guard let configuration = any as? ChatRichTextMessageConfiguration else { return }
        appliedConfiguration = configuration

        let hasAttachments = !configuration.attachmentItems.isEmpty || configuration.productLinkPreview != nil

        applyAttachments(configuration.attachmentItems)
        applyText(configuration.textViewModel, hasAttachments: hasAttachments)
        applyProductLinkPreview(configuration.productLinkPreview)

        invalidateIntrinsicContentSize()
    }

    private func applyProductLinkPreview(
        _ configuration: ChatProductLinkPreviewConfiguration?
    ) {
        guard let configuration else {
            productLinkPreviewView?.removeFromSuperview()
            productLinkPreviewView = nil
            return
        }

        if let productLinkPreviewView {
            productLinkPreviewView.configuration = configuration
        } else {
            let view = ChatProductLinkPreviewView(configuration: configuration)
            stackView.insertArrangedSubview(view, at: 0)
            productLinkPreviewView = view
        }
    }

    private func applyAttachments(
        _ items: [ChatRichTextMessageConfiguration.AttachmentItem]
    ) {
        guard !items.isEmpty else {
            mediaView?.removeFromSuperview()
            mediaView = nil
            return
        }

        if let mediaView {
            mediaView.configure(with: items)
        } else {
            let view = MediaAttachmentView()
            view.configure(with: items)
            stackView.insertArrangedSubview(view, at: 0)
            mediaView = view
        }
    }

    private func applyText(
        _ viewModel: ChatMessageTextView.ViewModel?,
        hasAttachments: Bool
    ) {
        let configuration: UIContentConfiguration? = viewModel.map { viewModel in
            SwiftUIContentConfiguration(
                view: ChatMessageTextView(viewModel: viewModel),
                margins: EdgeInsets(
                    top: 0,
                    leading: hasAttachments ? Layout.textLeadingInsetWithAttachments : 0,
                    bottom: 0,
                    trailing: hasAttachments ? Layout.textTrailingInsetWithAttachments : 0
                )
            )
        }

        textHostingView.apply(configuration) { [stackView] in
            stackView.addArrangedSubview($0)
        }
    }
}
