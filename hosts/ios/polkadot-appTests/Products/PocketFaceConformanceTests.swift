import Foundation
import PolkadotUI
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The faces a real product ships, decoded and drawn through the same path a
/// live face takes.
///
/// Both come from humanity-spa's `pocket:faces` and are checked against the
/// protocol on the product side, so a failure here is unambiguous: the defect
/// is this host's, not the file's.
///
/// Two of the nine it writes: `devicehood` is the shape most of the others
/// share node for node, and `candidate` is the large one. Keeping the identical
/// ones would add pages of fixture and no coverage.
@MainActor
struct PocketFaceConformanceTests {
    @Test(arguments: ["devicehood", "candidate"])
    func readsAndDrawsAPublishedFace(_ name: String) throws {
        let face = try parseRendererNodeJson(json: faceText(name))
        let drawn = try #require(face.toWidgetNode(resolver: WidgetDesignTokenResolver()))

        // Drawn to pixels, not just mapped: these faces build their surface out
        // of nodes sized only by their own modifiers, which map to a full tree
        // and can still reach the screen as one flat fill.
        let share = try WidgetNodeRaster.largestShare(of: drawn, in: cardSize)

        #expect(share < 0.9)
    }

    /// The bound the approval sheet applies, checked against the largest face a
    /// real product ships, so the limit is known to be above what products
    /// actually send rather than guessed.
    @Test
    func theLargestPublishedFaceFitsWellInsideTheSizeBound() throws {
        let bytes = try faceData("candidate").count

        #expect(bytes < PocketPreviewLoader.maxBytes / 2)
    }

    private var cardSize: CGSize {
        CGSize(width: 345, height: PocketCardSize.height)
    }

    private func faceData(_ name: String) throws -> Data {
        let url = try #require(
            Bundle(for: FaceFixtures.self).url(forResource: name, withExtension: "json"),
            "missing fixture \(name).json"
        )
        return try Data(contentsOf: url)
    }

    private func faceText(_ name: String) throws -> String {
        try String(decoding: faceData(name), as: UTF8.self)
    }
}

private final class FaceFixtures {}
