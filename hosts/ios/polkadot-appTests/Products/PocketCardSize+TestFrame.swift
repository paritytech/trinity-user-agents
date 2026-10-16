import CoreGraphics
@testable import polkadot_app

extension PocketCardSize {
    /// A card frame at a typical phone width, for drawing faces in tests.
    static let testFrame = CGSize(width: 393 - sideMargin * 2, height: height)
}
