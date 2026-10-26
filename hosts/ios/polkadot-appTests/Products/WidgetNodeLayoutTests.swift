import PolkadotUI
import SwiftUI
import Testing
import UIKit

/// The layout rules of `renderer.rs`, which Android draws the same way.
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

    /// No gap between children, and an arrangement only shares out space the
    /// stack was given: one with no size of its own hugs its children.
    @Test(arguments: [true, false])
    func aStackHugsItsChildrenWithNoGap(horizontal: Bool) async throws {
        let children = [block(.red, width: 20, height: 20), block(.green, width: 20, height: 20)]
        let stack = node(
            horizontal
                ? .row(.init(arrangement: .center), children: children)
                : .column(.init(arrangement: .center), children: children),
            background: .white
        )

        let drawn = try await WidgetNodeRaster.bounds(of: stack, in: CGSize(width: 100, height: 100))

        #expect(drawn[white] == nil)
        #expect(drawn[red]?.origin == .zero)
        #expect(drawn[green]?.origin == (horizontal ? CGPoint(x: 20, y: 0) : CGPoint(x: 0, y: 20)))
    }

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

    /// As large as its children, with the tint over them rather than hiding them.
    @Test
    func anEffectTintsItsChildrenAndTakesTheirSize() async throws {
        let row = node(.row(.init(), children: [
            node(.effect(.init(effect: .rainbow), children: [block(.red, width: 40, height: 40)])),
            block(.green, width: 20, height: 20),
        ]))

        let drawn = try await WidgetNodeRaster.bounds(of: row, in: CGSize(width: 100, height: 100))

        #expect(drawn[green]?.minX == 40)
        #expect(drawn[red] != nil)
    }

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

    @Test
    func aMissingImageDrawsItsPlaceholderInTheMiddleOfItsSpace() async throws {
        let image = node(.image(.init(source: .archive(path: "gone.png"))), width: 80, height: 60)

        let drawn = try await WidgetNodeRaster.bounds(
            of: image,
            in: CGSize(width: 100, height: 100),
            resolveImage: WidgetImageResolver { _ in nil }
        )
        let icon = drawn.filter { $0.key != WidgetNodeRaster.backdrop }.values.reduce(CGRect.null) { $0.union($1) }

        #expect(abs(icon.midX - 40) <= 1 && abs(icon.midY - 30) <= 1)
        #expect(icon.width <= 44 && icon.height <= 44)
    }

    /// Where a picture lands for each way a face can size it: cut to its node,
    /// never enlarged by ScaleDown, at its natural size shrunk to fit when the
    /// node has no size, and sized on one side by its proportions, or its
    /// natural size for None.
    @Test(arguments: [
        ImageCase(
            fit: .cover,
            width: 40,
            height: 40,
            picture: CGSize(width: 80, height: 40),
            drawn: CGRect(x: 0, y: 0, width: 40, height: 40)
        ),
        ImageCase(
            fit: .scaleDown,
            width: 100,
            height: 100,
            picture: CGSize(width: 30, height: 20),
            drawn: CGRect(x: 35, y: 40, width: 30, height: 20)
        ),
        ImageCase(
            fit: .contain,
            picture: CGSize(width: 30, height: 20),
            drawn: CGRect(x: 0, y: 0, width: 30, height: 20)
        ),
        ImageCase(
            fit: .contain,
            picture: CGSize(width: 300, height: 150),
            drawn: CGRect(x: 0, y: 0, width: 100, height: 50)
        ),
        ImageCase(
            fit: .cover,
            width: 40,
            picture: CGSize(width: 80, height: 40),
            drawn: CGRect(x: 0, y: 0, width: 40, height: 20)
        ),
        ImageCase(
            fit: .none,
            width: 40,
            picture: CGSize(width: 80, height: 40),
            drawn: CGRect(x: 0, y: 0, width: 40, height: 40)
        ),
    ])
    func anImageLandsWhereItsSizeAndFitPutIt(_ image: ImageCase) async throws {
        let imageNode = node(
            .image(.init(source: .archive(path: "art.png"), fit: image.fit)),
            width: image.width,
            height: image.height
        )

        let drawn = try await WidgetNodeRaster.bounds(
            of: imageNode,
            in: CGSize(width: 100, height: 100),
            resolveImage: picture(image.picture),
            until: { $0[red] != nil }
        )

        #expect(drawn[red] == image.drawn)
    }
}

// MARK: - Fixtures

struct ImageCase: Sendable {
    let fit: CustomMessageWidgetNode.ImageFit
    var width: CGFloat?
    var height: CGFloat?
    let picture: CGSize
    let drawn: CGRect
}

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
    static let white = Color(.sRGB, red: 1, green: 1, blue: 1)
}

/// A resolver that answers every source with one red picture of this many pixels.
private func picture(_ size: CGSize) -> WidgetImageResolver {
    let format = UIGraphicsImageRendererFormat()
    format.scale = 1
    let image = UIGraphicsImageRenderer(size: size, format: format).image { context in
        UIColor(red: 1, green: 0, blue: 0, alpha: 1).setFill()
        context.fill(CGRect(origin: .zero, size: size))
    }
    return WidgetImageResolver { _ in image }
}
