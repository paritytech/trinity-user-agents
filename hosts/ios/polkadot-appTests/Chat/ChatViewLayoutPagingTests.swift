import CoreGraphics
import PolkadotUI
import Testing

/// The scroll math of a chat that pages: the anchor that keeps a message in place and the 80 % trigger.
struct ChatViewLayoutPagingTests {
    // MARK: - Anchor

    @Test func aPageAboveKeepsTheVisibleMessageInPlace() {
        // The message is 40 points below the visible top; a page of 3000 points arrives above it.
        let anchor = ChatScrollAnchor(itemIdentifier: "m50", itemMinY: 1_240, contentOffsetY: 1_100, topInset: 100)

        let offset = anchor.contentOffsetY(
            itemMinY: 4_240,
            topInset: 100,
            bottomInset: 80,
            contentHeight: 9_000,
            boundsHeight: 800
        )

        #expect(anchor.offsetFromVisibleTop == 40)
        #expect(offset == 4_100)
        #expect(offset + 100 + anchor.offsetFromVisibleTop == 4_240)
    }

    @Test func theAnchorStaysWithinTheScrollRange() {
        let anchor = ChatScrollAnchor(itemIdentifier: "m1", offsetFromVisibleTop: 300)

        let belowTop = anchor.contentOffsetY(
            itemMinY: 100,
            topInset: 50,
            bottomInset: 0,
            contentHeight: 3_000,
            boundsHeight: 800
        )
        let pastBottom = anchor.contentOffsetY(
            itemMinY: 5_000,
            topInset: 50,
            bottomInset: 0,
            contentHeight: 3_000,
            boundsHeight: 800
        )

        #expect(belowTop == -50)
        #expect(pastBottom == 2_200)
    }

    @Test func aWindowThatDidNotMoveKeepsItsOffset() {
        let anchor = ChatScrollAnchor(itemIdentifier: "m1", itemMinY: 900, contentOffsetY: 700, topInset: 90)

        let offset = anchor.contentOffsetY(
            itemMinY: 900,
            topInset: 90,
            bottomInset: 34,
            contentHeight: 4_000,
            boundsHeight: 844
        )

        #expect(offset == 700)
    }

    // MARK: - Trigger

    @Test func olderPagesLoadPastEightyPercentOfTheWayUp() {
        // The range runs from -100 (top) to 2280 (bottom): 2380 points.
        let atBottom = trigger(offset: 2_280)
        let belowThreshold = trigger(offset: 2_280 - 2_380 * 0.79)
        let pastThreshold = trigger(offset: 2_280 - 2_380 * 0.81)
        let atTop = trigger(offset: -100)

        #expect(!atBottom.loadsOlder)
        #expect(!belowThreshold.loadsOlder)
        #expect(pastThreshold.loadsOlder)
        #expect(atTop.loadsOlder)
    }

    @Test func newerPagesLoadPastEightyPercentOfTheWayDown() {
        let atTop = trigger(offset: -100)
        let aboveThreshold = trigger(offset: 2_280 - 2_380 * 0.21)
        let pastThreshold = trigger(offset: 2_280 - 2_380 * 0.19)
        let atBottom = trigger(offset: 2_280)

        #expect(!atTop.loadsNewer)
        #expect(!aboveThreshold.loadsNewer)
        #expect(pastThreshold.loadsNewer)
        #expect(atBottom.loadsNewer)
    }

    @Test func aWindowThatFitsOnTheScreenAsksBothEnds() {
        let fits = ChatPagingTrigger(
            contentOffsetY: -100,
            topInset: 100,
            bottomInset: 80,
            contentHeight: 300,
            boundsHeight: 800
        )

        #expect(fits.loadsOlder)
        #expect(fits.loadsNewer)
    }

    @Test func theFractionScrolledUpRunsFromBottomToTop() {
        let range = ChatScrollRange(topInset: 0, bottomInset: 0, contentHeight: 1_800, boundsHeight: 800)

        #expect(range == ChatScrollRange(topInset: 0, bottomInset: 0, contentHeight: 1_800, boundsHeight: 800))
        #expect(range.maxOffsetY == 1_000)
        #expect(ChatPagingTrigger.fractionScrolledUp(contentOffsetY: 1_000, range: range) == 0)
        #expect(ChatPagingTrigger.fractionScrolledUp(contentOffsetY: 500, range: range) == 0.5)
        #expect(ChatPagingTrigger.fractionScrolledUp(contentOffsetY: -40, range: range) == 1)
    }
}

private func trigger(offset: CGFloat) -> ChatPagingTrigger {
    ChatPagingTrigger(
        contentOffsetY: offset,
        topInset: 100,
        bottomInset: 80,
        contentHeight: 3_000,
        boundsHeight: 800
    )
}
