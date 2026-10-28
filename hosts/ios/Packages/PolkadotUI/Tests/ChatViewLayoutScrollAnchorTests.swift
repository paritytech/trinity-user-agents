import SwiftUI
import Testing
import UIKit
@testable import PolkadotUI

@MainActor
struct ChatViewLayoutScrollAnchorTests {
    @Test("Cells that grow after the opening scroll keep the newest message on screen")
    func growingCellsKeepTheNewestMessageVisible() {
        let heights = CellHeights()
        let layout = makeLayout(heights: heights)
        pumpRunLoop()
        #expect(layout.visibleIdentifiers().last == Self.newestMessageId)

        heights.value = 300
        pumpRunLoop()

        #expect(layout.visibleIdentifiers().last == Self.newestMessageId)
    }

    @Test("Once the user scrolls, growing cells leave the list where the user put it")
    func userScrollReleasesTheAnchor() {
        let heights = CellHeights()
        let layout = makeLayout(heights: heights)
        pumpRunLoop()

        layout.scrollViewWillBeginDragging(layout.collectionView)
        layout.collectionView.contentOffset = .zero
        heights.value = 300
        pumpRunLoop()

        #expect(layout.visibleIdentifiers().first == "m0")
    }

    @Test("A status bar tap scrolls to the top without the anchor pulling the list back down")
    func statusBarTapReleasesTheAnchor() {
        let heights = CellHeights()
        let layout = makeLayout(heights: heights)
        pumpRunLoop()

        #expect(layout.scrollViewShouldScrollToTop(layout.collectionView))
        layout.collectionView.contentOffset = .zero
        heights.value = 300
        pumpRunLoop()

        #expect(layout.visibleIdentifiers().first == "m0")
    }

    @Test("A chat with unread messages opens with the first unread one in the middle of the screen")
    func firstUnreadMessageOpensCentred() {
        let layout = makeLayout(heights: CellHeights(), firstUnread: Self.firstUnreadMessageId)
        pumpRunLoop()

        let visible = layout.visibleIdentifiers()
        #expect(visible.contains(Self.firstUnreadMessageId))
        #expect(visible.first != Self.firstUnreadMessageId)
        #expect(visible.last != Self.firstUnreadMessageId)
    }

    @Test("Cells that grow after the opening scroll keep the first unread message on screen")
    func growingCellsKeepTheCentredItem() {
        let heights = CellHeights()
        let layout = makeLayout(heights: heights, firstUnread: Self.firstUnreadMessageId)
        pumpRunLoop()

        heights.value = 300
        pumpRunLoop()

        #expect(layout.visibleIdentifiers().contains(Self.firstUnreadMessageId))
    }

    @Test("Cells that grow during an animated scroll leave it to land before the anchor is restored")
    func animatedScrollLandsBeforeTheAnchorIsRestored() {
        let heights = CellHeights()
        let layout = makeLayout(
            heights: heights,
            firstUnread: Self.firstUnreadMessageId,
            reactionTarget: Self.firstUnreadMessageId
        )
        pumpRunLoop()

        // The target is already centred, so the animated scroll has no distance to cover
        // and UIKit never reports its end. Only the test does, once the cells have grown.
        tap(layout.scrollToReactionButton)
        heights.value = 300
        pumpRunLoop()
        #expect(!layout.visibleIdentifiers().contains(Self.firstUnreadMessageId))

        layout.scrollViewDidEndScrollingAnimation(layout.collectionView)

        #expect(layout.visibleIdentifiers().contains(Self.firstUnreadMessageId))
    }
}

private extension ChatViewLayoutScrollAnchorTests {
    static let messageCount = 40
    static var newestMessageId: String { "m\(messageCount - 1)" }
    static let firstUnreadMessageId = "m20"

    func makeLayout(
        heights: CellHeights,
        firstUnread: String? = nil,
        reactionTarget: String? = nil
    ) -> ChatViewLayout {
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 390, height: 700))
        let layout = ChatViewLayout(frame: window.bounds)
        window.addSubview(layout)
        window.makeKeyAndVisible()

        let messages = (0 ..< Self.messageCount).map { index in
            IdentifiableAnyContentConfiguration(
                id: "m\(index)",
                configuration: SwiftUIContentConfiguration(view: GrowingCell(id: "m\(index)", heights: heights))
            )
        }
        layout.bind(viewModel: ChatViewLayout.ViewModel(
            headerConfiguration: .empty(),
            chatInputConfiguration: nil,
            firstUnreadMessageIdentifier: firstUnread,
            scrollDownConfiguration: .init(),
            scrollToReactionConfiguration: .init(targetMessageId: reactionTarget),
            sections: [.init(identifier: "today", dateText: "Today", messages: messages)],
            footerConfiguration: nil
        ))
        return layout
    }

    /// Sends the touch up action straight to its targets.
    ///
    /// A package test runs without an application, so `sendActions(for:)` reaches no one.
    func tap(_ control: UIControl) {
        for case let target as NSObject in control.allTargets {
            for action in control.actions(forTarget: target, forControlEvent: .touchUpInside) ?? [] {
                _ = target.perform(Selector(action))
            }
        }
    }

    func pumpRunLoop() {
        for _ in 0 ..< 5 {
            RunLoop.main.run(until: Date().addingTimeInterval(0.05))
        }
    }
}

@Observable
private final class CellHeights {
    var value: CGFloat = 44
}

private struct GrowingCell: View, Hashable {
    let id: String
    let heights: CellHeights

    var body: some View {
        Color.gray.frame(height: heights.value)
    }

    static func == (lhs: Self, rhs: Self) -> Bool { lhs.id == rhs.id }
    func hash(into hasher: inout Hasher) { hasher.combine(id) }
}
