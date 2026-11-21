import PolkadotUI
import SwiftUI
import Testing
import UIKit
@testable import polkadot_app

/// A product that docks a slide to confirm keeps the same card in the footer while the slide runs and
/// turns its tree into an ordinary card when it is done. The footer crossfades only that hand-back.
@MainActor
struct ProductRoomFooterFaceTests {
    @MainActor
    @Observable
    final class Tree: WidgetNodeProviding {
        var node: CustomMessageWidgetNode?

        init(_ node: CustomMessageWidgetNode?) {
            self.node = node
        }
    }

    private static func slider(_ state: CustomMessageWidgetNode
        .SlideToConfirmState = .init()) -> CustomMessageWidgetNode {
        CustomMessageWidgetNode(
            content: .slideToConfirm(.init(
                text: "Slide to register",
                secondaryText: "Today at 20:00",
                confirmedText: "Registered",
                state: state,
                confirmAction: "register"
            )),
            modifiers: .init(height: 80, fillWidth: true)
        )
    }

    private static let registeredCard = CustomMessageWidgetNode(
        content: .column(.init(), children: [
            CustomMessageWidgetNode(content: .text(.init(
                text: "Registered",
                labelStyle: .body16Regular(),
                color: .primary
            ))),
            CustomMessageWidgetNode(content: .text(.init(
                text: "Today at 20:00",
                labelStyle: .body16Regular(),
                color: .primary
            ))),
        ]),
        modifiers: .init(height: 52)
    )

    @Test("A docked slider is the slide face")
    func sliderIsTheSlideFace() {
        #expect(ProductRoomFooterFace(node: Self.slider()) == .slide)
    }

    @Test("A slider inside a box, a column or a row is still the slide face")
    func nestedSliderIsTheSlideFace() {
        let inRow = CustomMessageWidgetNode(content: .row(.init(), children: [Self.slider()]))
        let inBox = CustomMessageWidgetNode(content: .box(
            .init(),
            children: [CustomMessageWidgetNode(content: .spacer), inRow]
        ))

        #expect(ProductRoomFooterFace(node: CustomMessageWidgetNode(content: .column(.init(), children: [inBox]))) ==
            .slide)
    }

    /// The slid control waiting on the product must keep its face, or the footer would crossfade the
    /// thumb away mid-wait.
    @Test("A slider waiting on the product, or confirmed, keeps the slide face")
    func waitingSliderKeepsTheSlideFace() {
        #expect(ProductRoomFooterFace(node: Self.slider(.init(isLoading: true))) == .slide)
        #expect(ProductRoomFooterFace(node: Self.slider(.init(isConfirmed: true))) == .slide)
    }

    @Test("The card a slide hands back to is the card face, and no tree yet is the drawing face")
    func handBackIsTheCardFace() {
        #expect(ProductRoomFooterFace(node: Self.registeredCard) == .card)
        #expect(ProductRoomFooterFace(node: nil) == .drawing)
    }

    /// The web footer's slot on iOS is 12 + 80 + 8: the docked card's padding around the pill. The
    /// native slider docked at 80 makes the same slot, so the two footers can be told apart only by
    /// what draws them.
    @Test("A docked slider at 80 makes a 100 slot, the web footer's height on iOS")
    func dockedSliderSlot() {
        let configuration = ProductRoomFooterCardConfiguration(
            roomId: "room",
            messageType: "game",
            payload: Data([1]),
            nodeProvider: Tree(Self.slider()),
            onAction: { _, _ in },
            resolveImage: WidgetImageResolver { _ in nil }
        )
        let view = configuration.makeContentView(for: nil)

        let size = view.systemLayoutSizeFitting(
            CGSize(width: 390, height: UIView.layoutFittingCompressedSize.height),
            withHorizontalFittingPriority: .required,
            verticalFittingPriority: .fittingSizeLevel
        )

        #expect(size.height == 100)
    }
}
