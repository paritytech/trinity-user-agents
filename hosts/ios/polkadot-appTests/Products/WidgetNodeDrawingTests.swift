import PolkadotUI
import SwiftUI
import Testing
import UIKit

/// Drawing, not mapping: a node can map to a full tree and still paint nothing.
///
/// Products draw a card's surface, its bars and its rules out of empty boxes
/// that take their size from their own modifiers, so a node the renderer leaves
/// unpainted costs a face most of what it shows.
@MainActor
struct WidgetNodeDrawingTests {
    @Test
    func aNodeSizedByItsOwnModifiersPaintsItsBackground() throws {
        let bar = CustomMessageWidgetNode(
            content: .box(CustomMessageWidgetNode.BoxProps(), children: []),
            modifiers: CustomMessageWidgetNode.Modifiers(
                background: CustomMessageWidgetNode.Background(color: .white),
                height: 40,
                fillWidth: true
            )
        )

        #expect(try whiteFraction(of: bar) > 0.9)
    }

    /// The same node with a shape, which takes the other branch of the
    /// background modifier and would otherwise go unchecked.
    @Test
    func aShapedBackgroundFollowsTheSizeTheNodeAsksFor() throws {
        let bar = CustomMessageWidgetNode(
            content: .box(CustomMessageWidgetNode.BoxProps(), children: []),
            modifiers: CustomMessageWidgetNode.Modifiers(
                background: CustomMessageWidgetNode.Background(
                    color: .white,
                    shape: AnyShape(Rectangle())
                ),
                height: 40,
                fillWidth: true
            )
        )

        #expect(try whiteFraction(of: bar) > 0.9)
    }

    /// The share of a 100x40 frame the node paints white, drawn over black.
    private func whiteFraction(of node: CustomMessageWidgetNode) throws -> Double {
        let size = CGSize(width: 100, height: 40)
        let counted = try WidgetNodeRaster.colours(of: node, in: size)
        let white = counted["3-3-3"] ?? 0

        return Double(white) / (size.width * size.height)
    }
}
