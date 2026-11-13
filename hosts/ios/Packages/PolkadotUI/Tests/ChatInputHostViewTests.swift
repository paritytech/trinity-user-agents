import SwiftUI
import Testing
import UIKit
@testable import PolkadotUI

@MainActor
struct ChatInputHostViewTests {
    @Test("Content that wraps is measured at the width of the host, not at its narrowest size")
    func measuresContentAtHostWidth() {
        // Given
        let text = String(repeating: "A docked card wraps its text across lines. ", count: 4)
        let card = UIHostingConfiguration { Text(text).font(.body) }.margins(.all, 0).makeContentView()
        let (window, container, host) = makeLaidOutHost(width: 200)

        // When
        host.bind(
            configuration: StubChatInputViewConfiguration(contentView: card),
            inputHandler: nil,
            keyboardGuide: container.keyboardLayoutGuide
        )
        window.layoutIfNeeded()

        // Then
        #expect(host.contentHeight > 2 * UIFont.preferredFont(forTextStyle: .body).lineHeight)
    }

    @Test("Content that grows after it is bound tells the handler on the next layout")
    func notifiesHandlerWhenContentGrowsAfterBind() {
        // Given
        let (content, heightConstraint) = makeFixedHeightContent(height: 60)
        let handler = SpyChatInputHandler()
        let (window, container, host) = makeLaidOutHost(width: 390)
        host.bind(
            configuration: StubChatInputViewConfiguration(contentView: content),
            inputHandler: handler,
            keyboardGuide: container.keyboardLayoutGuide
        )
        window.layoutIfNeeded()
        let changesAfterBind = handler.changeCount

        // When
        heightConstraint.constant = 140
        window.layoutIfNeeded()

        // Then
        #expect(handler.changeCount > changesAfterBind)
        #expect(host.contentHeight == 140)
    }

    @Test("A safe area fill colour does not add the safe area to the reported height")
    func fillColourLeavesHeightUnchanged() throws {
        // Given
        let (window, container, host) = makeLaidOutHost(width: 390)
        let keyboardGuide = container.keyboardLayoutGuide
        host.bind(
            configuration: StubChatInputViewConfiguration(contentView: makeFixedHeightContent(height: 60).view),
            inputHandler: nil,
            keyboardGuide: keyboardGuide
        )
        window.layoutIfNeeded()
        try #require(host.safeAreaInsets.bottom > 0)
        let plainHeight = host.contentHeight

        // When
        host.bind(
            configuration: StubChatInputViewConfiguration(
                contentView: makeFixedHeightContent(height: 60).view,
                safeAreaFillColor: .red
            ),
            inputHandler: nil,
            keyboardGuide: keyboardGuide
        )
        window.layoutIfNeeded()

        // Then
        #expect(plainHeight == 60)
        #expect(host.contentHeight == plainHeight)
    }

    @Test("A host that has no width yet reports no height")
    func reportsZeroBeforeItHasWidth() {
        // Given
        let container = UIView(frame: CGRect(x: 0, y: 0, width: 390, height: 800))
        let host = ChatInputHostView()
        container.addSubview(host)

        // When
        host.bind(
            configuration: StubChatInputViewConfiguration(contentView: makeFixedHeightContent(height: 60).view),
            inputHandler: nil,
            keyboardGuide: container.keyboardLayoutGuide
        )

        // Then
        #expect(host.bounds.width == 0)
        #expect(host.contentHeight == 0)
    }

    private func makeLaidOutHost(width: CGFloat) -> (UIWindow, UIView, ChatInputHostView) {
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: width, height: 800))
        let controller = UIViewController()
        controller.additionalSafeAreaInsets.bottom = 34
        window.rootViewController = controller
        window.isHidden = false
        let container: UIView = controller.view
        let host = ChatInputHostView()
        host.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(host)
        NSLayoutConstraint.activate([
            host.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            host.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            host.bottomAnchor.constraint(equalTo: container.bottomAnchor)
        ])
        window.layoutIfNeeded()
        return (window, container, host)
    }

    private func makeFixedHeightContent(height: CGFloat) -> (view: UIView, height: NSLayoutConstraint) {
        let view = UIView()
        let constraint = view.heightAnchor.constraint(equalToConstant: height)
        constraint.isActive = true
        return (view, constraint)
    }
}
