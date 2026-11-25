import UIKit

// MARK: - View model

public extension ChatViewLayout.ViewModel {
    /// What the chat can ask at each end of its window of messages.
    struct Paging: Equatable {
        public enum Edge: Equatable {
            /// No more messages exist past this end.
            case end
            /// More messages exist. The chat asks for them when the user scrolls near this end.
            case more
            /// A page for this end is on its way. The chat shows nothing for it.
            case loading
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
/// for newer messages.
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
