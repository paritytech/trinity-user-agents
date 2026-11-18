import Testing
import UIKit
@testable import PolkadotUI

/// Inline bytes come from a product, and a few hundred bytes can declare a picture of any size. Decoding them whole
/// would let a tiny file exhaust memory, so the picture is read at most 1024 pixels on its longer side.
struct InlineWidgetImageTests {
    @Test func decodesAHugeDeclaredPictureWithinTheBound() throws {
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        let wide = UIGraphicsImageRenderer(size: CGSize(width: 4_000, height: 400), format: format).pngData { context in
            context.fill(CGRect(x: 0, y: 0, width: 4_000, height: 400))
        }

        let image = try #require(UIImage(downsampling: wide))

        #expect(image.size.width * image.scale == 1_024)
    }

    @Test func drawsNothingForBytesThatAreNotAPicture() {
        #expect(UIImage(downsampling: Data("not a picture".utf8)) == nil)
    }
}
