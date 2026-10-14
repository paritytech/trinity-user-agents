import Testing
import UIKit

@testable import BlurHash

struct BlurHashTests {
    @Test("Fixture encodes to the Android-compatible wire value")
    func goldenEncodingVector() throws {
        let fixtureURL = try #require(
            Bundle.module.url(
                forResource: "blurhash-fixture",
                withExtension: "png"
            )
        )
        let source = try #require(UIImage(contentsOfFile: fixtureURL.path))
        let hash = try #require(source.blurHash(numberOfComponents: (4, 3)))

        #expect(hash == "L414LKkBUze:fkaffkknUgaflPkB")
    }

    @Test("Reference hash decodes to stable pixels")
    func goldenDecodingVector() throws {
        let blurHash = try #require(BlurHash("LEHV6nWB2yk8pyo0adR*.7kCMdnj"))
        let image = try #require(
            UIImage(
                blurHash: blurHash,
                size: CGSize(width: 4, height: 3)
            )
        )

        let topLeadingPixel = try rgbPixel(in: image, x: 0, y: 0)
        let centerPixel = try rgbPixel(in: image, x: 2, y: 1)
        let bottomTrailingPixel = try rgbPixel(in: image, x: 3, y: 2)

        #expect(topLeadingPixel == [135, 164, 177])
        #expect(centerPixel == [164, 145, 134])
        #expect(bottomTrailingPixel == [148, 140, 134])
    }

    @Test("Invalid hashes are rejected")
    func invalidHash() {
        #expect(BlurHash(rawValue: Data("not-a-blur-hash".utf8)) == nil)
    }

    @Test("Wire data round-trips through the typed boundary")
    func wireDataRoundTrip() throws {
        let value = "LEHV6nWB2yk8pyo0adR*.7kCMdnj"
        let blurHash = try #require(BlurHash(rawValue: Data(value.utf8)))

        #expect(blurHash.value == value)
        #expect(blurHash.toData() == Data(value.utf8))
    }

    private func rgbPixel(in image: UIImage, x pixelX: Int, y pixelY: Int) throws -> [UInt8] {
        let cgImage = try #require(image.cgImage)
        let data = try #require(cgImage.dataProvider?.data)
        let bytes = try #require(CFDataGetBytePtr(data))
        let offset = pixelY * cgImage.bytesPerRow + pixelX * 3
        return [bytes[offset], bytes[offset + 1], bytes[offset + 2]]
    }
}
