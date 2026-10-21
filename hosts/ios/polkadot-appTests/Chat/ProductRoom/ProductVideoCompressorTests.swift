import AVFoundation
import Foundation
import Testing
@testable import polkadot_app

/// A video above the inline limit is exported at 640x480 before it goes to a product room (B5 of the verbosity
/// product plan).
struct ProductVideoCompressorTests {
    @Test func aLargeVideoBecomesAnMP4At640x480UnderTheLimit() async throws {
        let video = try await Self.noiseVideo(seconds: 2)
        #expect(video.count > ProductAttachmentEncoder.inlineLimit)

        let smaller = try await ProductVideoCompressor.compress(video, limit: ProductAttachmentEncoder.inlineLimit)

        #expect(smaller.count <= ProductAttachmentEncoder.inlineLimit)
        let size = try await Self.naturalSize(of: smaller)
        #expect(max(size.width, size.height) <= 640)
        #expect(min(size.width, size.height) <= 480)
    }

    @Test func aVideoThatStaysAboveTheLimitIsRefusedWithItsSize() async throws {
        let video = try await Self.noiseVideo(seconds: 1)

        do {
            _ = try await ProductVideoCompressor.compress(video, limit: 1_000)
            Issue.record("a video above the limit is refused")
        } catch let ProductVideoCompressor.Failure.tooLarge(bytes, limit) {
            #expect(bytes > 1_000)
            #expect(limit == 1_000)
        }
    }

    @Test func bytesThatAreNoVideoAreUnreadable() async {
        await #expect(throws: ProductVideoCompressor.Failure.unreadable) {
            try await ProductVideoCompressor.compress(Data([1, 2, 3]), limit: 1_000)
        }
    }
}

extension ProductVideoCompressorTests {
    /// A 1280x720 H.264 video of random pixels at a high bit rate, so that it is larger than the inline limit.
    static func noiseVideo(seconds: Int) async throws -> Data {
        let url = FileManager.default.temporaryDirectory.appending(path: "noise-\(UUID().uuidString).mov")
        defer { try? FileManager.default.removeItem(at: url) }
        let writer = try AVAssetWriter(outputURL: url, fileType: .mov)
        let input = AVAssetWriterInput(mediaType: .video, outputSettings: [
            AVVideoCodecKey: AVVideoCodecType.h264,
            AVVideoWidthKey: 1_280,
            AVVideoHeightKey: 720,
            AVVideoCompressionPropertiesKey: [AVVideoAverageBitRateKey: 20_000_000]
        ])
        let adaptor = AVAssetWriterInputPixelBufferAdaptor(assetWriterInput: input, sourcePixelBufferAttributes: [
            kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA,
            kCVPixelBufferWidthKey as String: 1_280,
            kCVPixelBufferHeightKey as String: 720
        ])
        writer.add(input)
        writer.startWriting()
        writer.startSession(atSourceTime: .zero)
        var generator = SystemRandomNumberGenerator()
        for frame in 0 ..< seconds * 30 {
            while !input.isReadyForMoreMediaData {
                try await Task.sleep(for: .milliseconds(5))
            }
            let buffer = try #require(adaptor.pixelBufferPool.flatMap { pool -> CVPixelBuffer? in
                var buffer: CVPixelBuffer?
                CVPixelBufferPoolCreatePixelBuffer(nil, pool, &buffer)
                return buffer
            })
            CVPixelBufferLockBaseAddress(buffer, [])
            let base = try #require(CVPixelBufferGetBaseAddress(buffer))
            let count = CVPixelBufferGetDataSize(buffer)
            let words = base.bindMemory(to: UInt64.self, capacity: count / 8)
            for index in 0 ..< count / 8 {
                words[index] = generator.next()
            }
            CVPixelBufferUnlockBaseAddress(buffer, [])
            adaptor.append(buffer, withPresentationTime: CMTime(value: CMTimeValue(frame), timescale: 30))
        }
        input.markAsFinished()
        await writer.finishWriting()
        return try Data(contentsOf: url)
    }

    static func naturalSize(of video: Data) async throws -> CGSize {
        let url = FileManager.default.temporaryDirectory.appending(path: "check-\(UUID().uuidString).mp4")
        defer { try? FileManager.default.removeItem(at: url) }
        try video.write(to: url)
        let track = try #require(try await AVURLAsset(url: url).loadTracks(withMediaType: .video).first)
        return try await track.load(.naturalSize)
    }
}
