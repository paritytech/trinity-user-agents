import Foundation
import Testing
import UIKit
import UIKitExt
@testable import polkadot_app

@MainActor
struct WidgetViewTests {
    private let cardInset: CGFloat = 300

    @Test func reportsThePageScrollOffset() {
        let widgetView = WidgetView(cardInset: cardInset, schemeHandler: SchemeHandlerProxy())
        var offsets: [CGFloat] = []
        widgetView.onScroll = { offsets.append($0) }

        widgetView.webView.scrollView.contentOffset = CGPoint(x: 0, y: 120)

        #expect(offsets.last == 120)
    }

    @Test func failureLeavesTheCardAreaToTheFace() throws {
        let widgetView = WidgetView(cardInset: cardInset, schemeHandler: SchemeHandlerProxy())
        widgetView.frame = CGRect(x: 0, y: 0, width: 390, height: 844)

        widgetView.showFailure(ErrorContent(title: "Failed", message: "Try again"))
        widgetView.layoutIfNeeded()

        let failureView = try #require(widgetView.subviews.first { $0 is UIContentUnavailableView })
        #expect(failureView.frame.minY == cardInset)
    }
}
