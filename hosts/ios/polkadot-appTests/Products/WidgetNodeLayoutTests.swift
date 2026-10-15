import PolkadotUI
import SwiftUI
import Testing
import UIKit

/// The layout rules a face is written against, which Android draws the same
/// way: one tree has to land on the same pixels on both.
@MainActor
struct WidgetNodeLayoutTests {
    @Test
    func paddingGoesAroundAFixedSize() async throws {
        let padded = block(
            .red,
            width: 40,
            height: 20,
            padding: EdgeInsets(top: 0, leading: 0, bottom: 20, trailing: 0)
        )

        let drawn = try await WidgetNodeRaster.bounds(of: padded, in: CGSize(width: 100, height: 100))

        #expect(drawn[red] == CGRect(x: 0, y: 0, width: 40, height: 40))
    }

    @Test
    func aBoxPlacesItsChildrenAtTheTopStartByDefault() async throws {
        let box = node(.box(.init(), children: [block(.red, width: 20, height: 20)]), width: 100, height: 60)

        let drawn = try await WidgetNodeRaster.bounds(of: box, in: CGSize(width: 100, height: 60))

        #expect(drawn[red]?.origin == .zero)
    }

    /// Space between children comes only from a Spacer or an arrangement.
    @Test(arguments: [true, false])
    func aStackAddsNoGapBetweenItsChildren(horizontal: Bool) async throws {
        let children = [block(.red, width: 20, height: 20), block(.green, width: 20, height: 20)]
        let stack = node(horizontal ? .row(.init(), children: children) : .column(.init(), children: children))

        let drawn = try await WidgetNodeRaster.bounds(of: stack, in: CGSize(width: 100, height: 100))

        #expect(drawn[green]?.origin == (horizontal ? CGPoint(x: 20, y: 0) : CGPoint(x: 0, y: 20)))
    }

    /// An arrangement shares out space the stack was given, so a stack that
    /// hugs its children has none to share.
    @Test(arguments: [true, false])
    func anArrangementOnlyMovesChildrenWithinSpareSpace(horizontal: Bool) async throws {
        let children = [block(.red, width: 20, height: 20)]
        let stack = node(
            horizontal
                ? .row(.init(arrangement: .center), children: children)
                : .column(.init(arrangement: .center), children: children),
            background: .white
        )

        let drawn = try await WidgetNodeRaster.bounds(of: stack, in: CGSize(width: 100, height: 100))

        #expect(drawn[white] == nil)
        #expect(drawn[red]?.origin == .zero)
    }

    /// Half a gap at each end and a whole one between, as the name says.
    @Test
    func spaceAroundLeavesHalfAGapAtEachEnd() async throws {
        let row = node(
            .row(
                .init(arrangement: .spaceAround),
                children: [block(.red, width: 20, height: 20), block(.green, width: 20, height: 20)]
            ),
            width: 100
        )

        let drawn = try await WidgetNodeRaster.bounds(of: row, in: CGSize(width: 100, height: 40))

        #expect([drawn[red]?.minX, drawn[green]?.minX] == [15, 65])
    }

    /// Takes no more room than its children, wherever it is placed.
    @Test
    func anEffectIsAsLargeAsItsChildren() async throws {
        let row = node(.row(.init(), children: [
            node(.effect(.init(effect: .rainbow), children: [block(.blue, width: 40, height: 40)])),
            block(.green, width: 20, height: 20),
        ]))

        let drawn = try await WidgetNodeRaster.bounds(of: row, in: CGSize(width: 100, height: 100))

        #expect(drawn[green]?.minX == 40)
    }

    /// The tint goes over the children, so what they draw still shows.
    @Test
    func anEffectTintsItsChildrenRatherThanHidingThem() async throws {
        let effect = node(.effect(.init(effect: .rainbow), children: [block(.red, width: 40, height: 40)]))

        let drawn = try await WidgetNodeRaster.bounds(of: effect, in: CGSize(width: 40, height: 40))

        #expect(drawn[red] != nil)
    }

    /// A border inside the node never covers its neighbour.
    @Test
    func aBorderIsDrawnInsideTheNode() async throws {
        let bordered = node(
            .box(.init(), children: []),
            border: CustomMessageWidgetNode.Border(color: .white, width: 4),
            width: 20,
            height: 20
        )
        let row = node(.row(.init(), children: [bordered, block(.green, width: 20, height: 20)]))

        let drawn = try await WidgetNodeRaster.bounds(of: row, in: CGSize(width: 60, height: 40))

        #expect(drawn[white] == CGRect(x: 0, y: 0, width: 20, height: 20))
    }

    /// As wide as its label, unless the node is given a width.
    @Test(arguments: [false, true])
    func aButtonIsAsWideAsItsLabelUnlessSized(filled: Bool) async throws {
        let button = CustomMessageWidgetNode(
            content: .button(.init(text: "Tap")),
            modifiers: CustomMessageWidgetNode.Modifiers(fillWidth: filled)
        )

        let drawn = try await WidgetNodeRaster.bounds(of: button, in: CGSize(width: 300, height: 60))
        let painted = drawn.filter { $0.key != WidgetNodeRaster.backdrop }.values.reduce(CGRect.null) { $0.union($1) }

        #expect(filled ? painted.width == 300 : painted.width < 150)
    }

    /// An image that cannot be loaded leaves its space empty.
    @Test
    func aMissingImageDrawsNothing() async throws {
        let image = node(.image(.init(source: .archive(path: "gone.png"))), width: 40, height: 40)

        let drawn = try await WidgetNodeRaster.bounds(
            of: image,
            in: CGSize(width: 40, height: 40),
            resolveImage: WidgetImageResolver { _ in nil }
        )

        #expect(Array(drawn.keys) == [WidgetNodeRaster.backdrop])
    }

    @Test
    func aCoverImageStaysInsideItsBox() async throws {
        let row = node(.row(.init(), children: [
            block(.green, width: 20, height: 40),
            node(.image(.init(source: .archive(path: "wide.png"), fit: .cover)), width: 40, height: 40),
            block(.green, width: 20, height: 40),
        ]))

        let drawn = try await WidgetNodeRaster.bounds(
            of: row,
            in: CGSize(width: 100, height: 40),
            resolveImage: picture(width: 80, height: 40),
            until: { $0[red] != nil }
        )

        #expect(drawn[red] == CGRect(x: 20, y: 0, width: 40, height: 40))
    }

    @Test
    func scaleDownNeverEnlargesAnImage() async throws {
        let image = node(.image(.init(source: .archive(path: "small.png"), fit: .scaleDown)), width: 100, height: 100)

        let drawn = try await WidgetNodeRaster.bounds(
            of: image,
            in: CGSize(width: 100, height: 100),
            resolveImage: picture(width: 30, height: 20),
            until: { $0[red] != nil }
        )

        #expect(drawn[red]?.size == CGSize(width: 30, height: 20))
    }

    /// One pixel to a point, shrunk to fit when larger than the space given.
    @Test(arguments: [
        (CGSize(width: 30, height: 20), CGSize(width: 30, height: 20)),
        (CGSize(width: 300, height: 150), CGSize(width: 100, height: 50)),
    ])
    func anImageWithoutASizeTakesItsNaturalSize(pictureSize: CGSize, drawnSize: CGSize) async throws {
        let image = node(.image(.init(source: .archive(path: "small.png"), fit: .contain)))

        let drawn = try await WidgetNodeRaster.bounds(
            of: image,
            in: CGSize(width: 100, height: 100),
            resolveImage: picture(width: pictureSize.width, height: pictureSize.height),
            until: { $0[red] != nil }
        )

        #expect(drawn[red]?.origin == .zero)
        #expect(drawn[red]?.size == drawnSize)
    }

    /// An image sized on one side never stretches to the space around it: it
    /// takes the other side from its proportions, or its natural size when it
    /// is not to be resized.
    @Test(arguments: [(CustomMessageWidgetNode.ImageFit.cover, 20.0), (.none, 40.0)])
    func anImageSizedOnOneSideKeepsItsOwnHeight(fit: CustomMessageWidgetNode.ImageFit, height: CGFloat) async throws {
        let column = node(.column(.init(), children: [
            node(.image(.init(source: .archive(path: "wide.png"), fit: fit)), width: 40),
            block(.green, width: 20, height: 20),
        ]))

        let drawn = try await WidgetNodeRaster.bounds(
            of: column,
            in: CGSize(width: 100, height: 100),
            resolveImage: picture(width: 80, height: 40),
            until: { $0[red] != nil }
        )

        #expect(drawn[green]?.minY == height)
    }
}

// MARK: - Fixtures

private let red = "3-0-0"
private let green = "0-3-0"
private let white = "3-3-3"

private func node(
    _ content: CustomMessageWidgetNode.Content,
    background: Color? = nil,
    border: CustomMessageWidgetNode.Border? = nil,
    width: CGFloat? = nil,
    height: CGFloat? = nil,
    padding: EdgeInsets = EdgeInsets()
) -> CustomMessageWidgetNode {
    CustomMessageWidgetNode(
        content: content,
        modifiers: CustomMessageWidgetNode.Modifiers(
            padding: padding,
            background: background.map { CustomMessageWidgetNode.Background(color: $0) },
            border: border,
            width: width,
            height: height
        )
    )
}

private func block(
    _ color: Color,
    width: CGFloat,
    height: CGFloat,
    padding: EdgeInsets = EdgeInsets()
) -> CustomMessageWidgetNode {
    node(.box(.init(), children: []), background: color, width: width, height: height, padding: padding)
}

/// Pure colours, so each lands in one bucket of its own.
private extension Color {
    static let red = Color(.sRGB, red: 1, green: 0, blue: 0)
    static let green = Color(.sRGB, red: 0, green: 1, blue: 0)
    static let blue = Color(.sRGB, red: 0, green: 0, blue: 1)
    static let white = Color(.sRGB, red: 1, green: 1, blue: 1)
}

/// A resolver that answers every source with one red picture of this many pixels.
private func picture(width: CGFloat, height: CGFloat) -> WidgetImageResolver {
    let format = UIGraphicsImageRendererFormat()
    format.scale = 1
    let image = UIGraphicsImageRenderer(size: CGSize(width: width, height: height), format: format).image { context in
        UIColor(red: 1, green: 0, blue: 0, alpha: 1).setFill()
        context.fill(CGRect(x: 0, y: 0, width: width, height: height))
    }
    return WidgetImageResolver { _ in image }
}
