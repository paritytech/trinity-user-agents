import DesignSystem
import UIKit
internal import SnapKit

// MARK: - View model

public extension ChatViewLayout.ViewModel {
    /// What the chat can ask at each end of its window of messages (P5.3).
    struct Paging: Equatable {
        public enum Edge: Equatable {
            /// No more messages exist past this end.
            case end
            /// More messages exist. The chat asks for them when the user scrolls near this end.
            case more
            /// A page for this end is on its way. The chat shows a row with a spinner.
            case loading
            /// The last page did not arrive. The row shows the text, and a tap asks again.
            case failed(String)
        }

        public let older: Edge
        public let newer: Edge

        public init(older: Edge, newer: Edge) {
            self.older = older
            self.newer = newer
        }

        /// A window that holds the whole chat.
        public static let complete = Paging(older: .end, newer: .end)
    }

    /// Whether the product of the room can answer (P5.7).
    enum RoomStatus: Equatable {
        /// The first page is on its way.
        case loading(String)
        /// The product does not answer. The button asks again.
        case unavailable(text: String, retryTitle: String)
    }
}

// MARK: - Scroll anchor

/// Keeps a message at the same place on the screen while messages arrive above it.
public struct ChatScrollAnchor: Equatable {
    /// The item that stays in place.
    public let itemIdentifier: String
    /// The distance from the top of the visible area to the top of the item.
    public let offsetFromVisibleTop: CGFloat

    public init(itemIdentifier: String, offsetFromVisibleTop: CGFloat) {
        self.itemIdentifier = itemIdentifier
        self.offsetFromVisibleTop = offsetFromVisibleTop
    }

    /// The anchor of the item from its frame before the update.
    public init(itemIdentifier: String, itemMinY: CGFloat, contentOffsetY: CGFloat, topInset: CGFloat) {
        self.init(
            itemIdentifier: itemIdentifier,
            offsetFromVisibleTop: itemMinY - (contentOffsetY + topInset)
        )
    }

    /// The content offset that puts the item at the same distance from the visible top after the update.
    ///
    /// The result stays in the range that the scroll view allows.
    public func contentOffsetY(
        itemMinY: CGFloat,
        topInset: CGFloat,
        bottomInset: CGFloat,
        contentHeight: CGFloat,
        boundsHeight: CGFloat
    ) -> CGFloat {
        let wanted = itemMinY - offsetFromVisibleTop - topInset
        let range = ChatScrollRange(
            topInset: topInset,
            bottomInset: bottomInset,
            contentHeight: contentHeight,
            boundsHeight: boundsHeight
        )
        return min(max(wanted, range.minOffsetY), range.maxOffsetY)
    }
}

/// The content offsets that a vertical scroll view allows.
public struct ChatScrollRange: Equatable {
    public let minOffsetY: CGFloat
    public let maxOffsetY: CGFloat

    public init(topInset: CGFloat, bottomInset: CGFloat, contentHeight: CGFloat, boundsHeight: CGFloat) {
        minOffsetY = -topInset
        maxOffsetY = max(contentHeight + bottomInset - boundsHeight, -topInset)
    }

    /// True when all the content is visible, so the user cannot scroll.
    public var fitsOnScreen: Bool {
        maxOffsetY <= minOffsetY
    }
}

// MARK: - Paging trigger

/// When the chat asks for the next page: 80 % of the way up for older messages, and 80 % of the way down
/// for newer messages (02-DESIGN §8.3).
public struct ChatPagingTrigger: Equatable {
    public static let threshold: CGFloat = 0.8

    public let loadsOlder: Bool
    public let loadsNewer: Bool

    public init(
        contentOffsetY: CGFloat,
        topInset: CGFloat,
        bottomInset: CGFloat,
        contentHeight: CGFloat,
        boundsHeight: CGFloat
    ) {
        let range = ChatScrollRange(
            topInset: topInset,
            bottomInset: bottomInset,
            contentHeight: contentHeight,
            boundsHeight: boundsHeight
        )

        // A window that fits on the screen asks both ends, so the screen fills.
        guard !range.fitsOnScreen else {
            loadsOlder = true
            loadsNewer = true
            return
        }

        let scrolledUp = Self.fractionScrolledUp(contentOffsetY: contentOffsetY, range: range)
        loadsOlder = scrolledUp >= Self.threshold
        loadsNewer = scrolledUp <= 1 - Self.threshold
    }

    /// 0 at the bottom of the content and 1 at the top.
    public static func fractionScrolledUp(contentOffsetY: CGFloat, range: ChatScrollRange) -> CGFloat {
        let span = range.maxOffsetY - range.minOffsetY
        guard span > 0 else {
            return 1
        }

        return min(max((range.maxOffsetY - contentOffsetY) / span, 0), 1)
    }
}

// MARK: - Paging row

/// The row at an end of the window while a page is on its way, or after a page failed.
public struct ChatPagingRowConfiguration: HashableContentConfiguration {
    public enum State: Hashable {
        case loading
        case failed(String)
    }

    let state: State

    public init(state: State) {
        self.state = state
    }

    public func makeContentView() -> any UIView & UIContentView {
        ChatPagingRowView(configuration: self)
    }
}

final class ChatPagingRowView: UIView, UIContentView {
    private let spinner = UIActivityIndicatorView(style: .medium)
    private let label: Label = create {
        $0.typography = .paragraphSmall
        $0.textColor = .fgSecondary
        $0.textAlignment = .center
        $0.numberOfLines = 0
    }

    var configuration: UIContentConfiguration {
        didSet {
            apply()
        }
    }

    init(configuration: ChatPagingRowConfiguration) {
        self.configuration = configuration
        super.init(frame: .zero)

        addSubview(spinner)
        addSubview(label)
        spinner.snp.makeConstraints {
            $0.center.equalToSuperview()
            $0.top.greaterThanOrEqualToSuperview().inset(DSSpacings.small)
        }
        label.snp.makeConstraints {
            $0.edges.equalToSuperview().inset(DSSpacings.small)
        }
        apply()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func apply() {
        guard let configuration = configuration as? ChatPagingRowConfiguration else {
            return
        }

        switch configuration.state {
        case .loading:
            label.text = nil
            label.isHidden = true
            spinner.startAnimating()
        case let .failed(text):
            spinner.stopAnimating()
            label.text = text
            label.isHidden = false
        }
    }
}

// MARK: - Room status

/// The line above the input while the product of the room loads or does not answer (P5.7).
final class ChatRoomStatusView: UIView {
    var onRetry: (() -> Void)?

    private let spinner = UIActivityIndicatorView(style: .medium)
    private let label: Label = create {
        $0.typography = .paragraphSmall
        $0.textColor = .fgSecondary
        $0.numberOfLines = 0
    }

    private let retryButton = UIButton(type: .system)

    var status: ChatViewLayout.ViewModel.RoomStatus? {
        didSet {
            apply()
        }
    }

    override init(frame: CGRect) {
        super.init(frame: frame)

        backgroundColor = .bgSurfaceMain
        let stack = UIStackView(arrangedSubviews: [spinner, label, retryButton])
        stack.axis = .horizontal
        stack.alignment = .center
        stack.spacing = DSSpacings.small
        addSubview(stack)
        stack.snp.makeConstraints {
            $0.edges.equalToSuperview().inset(DSSpacings.small)
        }

        retryButton.addTarget(self, action: #selector(handleRetry), for: .touchUpInside)
        apply()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func apply() {
        switch status {
        case .none:
            isHidden = true
            spinner.stopAnimating()
        case let .loading(text):
            isHidden = false
            spinner.startAnimating()
            label.text = text
            retryButton.isHidden = true
        case let .unavailable(text, retryTitle):
            isHidden = false
            spinner.stopAnimating()
            label.text = text
            retryButton.setTitle(retryTitle, for: .normal)
            retryButton.isHidden = false
        }
    }

    @objc
    private func handleRetry() {
        onRetry?()
    }
}
