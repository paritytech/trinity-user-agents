import Foundation
import Testing
@testable import polkadot_app

/// A video that the user sends to a product carries the size and the BlurHash of its first frame, so the room of the
/// contact shows a poster until the video is there (PARITY-NATIVE-IOS D7).
struct ProductVideoFactsTests {
    @Test func aVideoGivesTheSizeAndTheBlurHashOfItsFirstFrame() async throws {
        let video = try await ProductVideoCompressorTests.noiseVideo(seconds: 1)

        let inline = try #require(InlineDataURL("data:video/quicktime;base64,\(video.base64EncodedString())"))
        let meta = inline.mediaMeta

        #expect(meta.width == 1_280)
        #expect(meta.height == 720)
        #expect(meta.placeholder?.isEmpty == false)
    }

    @Test func bytesThatAreNoVideoGiveNoFacts() {
        #expect(ProductVideoFacts(Data([1, 2, 3]), mimeType: "video/mp4") == nil)
    }
}
