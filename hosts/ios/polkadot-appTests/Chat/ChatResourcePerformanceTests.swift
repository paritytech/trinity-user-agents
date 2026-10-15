import Foundation
import Testing
import UIKit
@testable import polkadot_app

/// The resource checks of P7.1 (04-TESTING §5): B4, a picture of 1 MiB decoded away from the main thread, and
/// B6, the disk cache bounded at 64 MiB.
///
/// The tests that measure run only when the test process has `TRUAPI_CHAT_PERF=1` (`TEST_RUNNER_TRUAPI_CHAT_PERF=1`
/// on the `xcodebuild` command line). They write `[perf]` lines with the numbers, to the file that
/// `TRUAPI_CHAT_PERF_OUT` names when it is set.
final class ChatResourcePerformanceTests {
    private static let measures = ProcessInfo.processInfo.environment["TRUAPI_CHAT_PERF"] == "1"
    private let directory = URL.temporaryDirectory.appending(path: "chat-resource-perf-\(UUID().uuidString)")

    deinit {
        try? FileManager.default.removeItem(at: directory)
    }

    /// The resolver gives a decoded bitmap, and a picture larger than the bound is scaled down while it decodes.
    @Test func aLargePictureDecodesToABoundedBitmap() throws {
        let data = try #require(Self.picture(width: 3_000, height: 1_500, quality: 0.5))

        let image = try #require(ChatResourceResolver.decodedImage(from: data))

        let bitmap = try #require(image.cgImage)
        #expect(max(bitmap.width, bitmap.height) == ChatResourceResolver.maxDecodedPixelSize)
        #expect(bitmap.width == 2 * bitmap.height)
    }

    @Test func aSmallPictureKeepsItsSize() throws {
        let data = try #require(Self.picture(width: 40, height: 20, quality: 0.9))

        let image = try #require(ChatResourceResolver.decodedImage(from: data))

        #expect(image.cgImage?.width == 40)
        #expect(image.cgImage?.height == 20)
    }

    /// A new cache on a directory that holds files counts them, and removes the oldest past its bound.
    @Test func aNewCacheCountsTheFilesThatTheDirectoryHolds() async throws {
        let shared = directory.appending(path: "shared")
        let first = ChatResourceDiskCache(directory: shared, maxBytes: 10)
        await first.store(Data(repeating: 1, count: 6), for: "old")
        try await Task.sleep(for: .milliseconds(20))

        let second = ChatResourceDiskCache(directory: shared, maxBytes: 10)
        await second.store(Data(repeating: 2, count: 6), for: "new")

        #expect(await second.data(for: "old") == nil)
        #expect(await second.data(for: "new") == Data(repeating: 2, count: 6))
    }

    /// B4: the resolver decodes a picture of 1 MiB on its own task, so the main thread receives a bitmap. A
    /// `UIImage(data:)` defers the decode to its first draw, which is on the main thread.
    @Test(.enabled(if: measures), arguments: [(3_000, 2_250), (2_048, 1_536)])
    func b4AOneMebibytePictureDecodesAwayFromTheMainThread(width: Int, height: Int) async throws {
        let data = try #require(Self.pictureOfOneMebibyte(width: width, height: height))
        let resource = ChatResource.inline(mimeType: "image/jpeg", data: data)

        var resolveTimes: [Double] = []
        var deferredDecodes: [Double] = []
        var decodedOnMain: [Bool] = []
        for _ in 0 ..< 5 {
            let resolver = makeResolver()
            let start = ContinuousClock.now
            let image = try #require(await resolver.image(for: resource))
            resolveTimes.append(Self.milliseconds(since: start))
            decodedOnMain.append(image.cgImage == nil)

            try await deferredDecodes.append(MainActor.run {
                let lazy = try #require(UIImage(data: data))
                let start = ContinuousClock.now
                _ = lazy.preparingForDisplay()
                return Self.milliseconds(since: start)
            })
        }

        let pixels = try #require(UIImage(data: data)).size
        report(
            "[perf] B4 picture bytes=\(data.count) pixels=\(Int(pixels.width))x\(Int(pixels.height))"
                + " resolve+decode on the resolver task ms median=\(Self.median(resolveTimes))"
                + " max=\(Self.format(resolveTimes.max() ?? 0))"
                + "; the decode that UIImage(data:) defers to the main thread ms median=\(Self.median(deferredDecodes))"
        )
        #expect(!decodedOnMain.contains(true))
    }

    /// B6: the disk cache stays within 64 MiB, and removes the files that were read longest ago.
    @Test(.enabled(if: measures)) func b6TheDiskCacheStaysWithinItsBound() async throws {
        let bound = 64 * 1_024 * 1_024
        let large = ChatResourceDiskCache(directory: directory.appending(path: "large"), maxBytes: bound)
        let mebibyte = Data(repeating: 7, count: 1_024 * 1_024)
        var largeStores: [Double] = []
        for index in 0 ..< 100 {
            let start = ContinuousClock.now
            await large.store(mebibyte, for: "large-\(index)")
            largeStores.append(Self.milliseconds(since: start))
        }
        let largeTotal = try Self.directorySize(directory.appending(path: "large"))
        let oldestKept = await large.data(for: "large-0") != nil
        let newestKept = await large.data(for: "large-99") != nil

        let small = ChatResourceDiskCache(directory: directory.appending(path: "small"), maxBytes: bound)
        let piece = Data(repeating: 3, count: 32 * 1_024)
        var smallStores: [Double] = []
        for index in 0 ..< 2_500 {
            let start = ContinuousClock.now
            await small.store(piece, for: "small-\(index)")
            smallStores.append(Self.milliseconds(since: start))
        }
        let smallTotal = try Self.directorySize(directory.appending(path: "small"))

        report(
            "[perf] B6 100 x 1 MiB: total MiB=\(largeTotal / 1_048_576) store ms median=\(Self.median(largeStores))"
                + " max=\(Self.format(largeStores.max() ?? 0)); 2,500 x 32 KiB: total MiB=\(smallTotal / 1_048_576)"
                + " store ms median=\(Self.median(smallStores)) max=\(Self.format(smallStores.max() ?? 0))"
        )
        #expect(largeTotal <= bound)
        #expect(smallTotal <= bound)
        #expect(!oldestKept)
        #expect(newestKept)
    }
}

private extension ChatResourcePerformanceTests {
    /// Prints a line, and adds it to the file of `TRUAPI_CHAT_PERF_OUT`.
    func report(_ line: String) {
        print(line)
        guard let path = ProcessInfo.processInfo.environment["TRUAPI_CHAT_PERF_OUT"],
              let data = (line + "\n").data(using: .utf8) else { return }
        if let handle = FileHandle(forWritingAtPath: path) {
            handle.seekToEndOfFile()
            handle.write(data)
            try? handle.close()
        } else {
            FileManager.default.createFile(atPath: path, contents: data)
        }
    }

    func makeResolver() -> ChatResourceResolver {
        ChatResourceResolver(
            preimages: NoPreimages(),
            cache: ChatResourceDiskCache(directory: directory.appending(path: "resolver"), maxBytes: 1_024),
            fetch: { _ in Data() }
        )
    }

    /// A photo-like JPEG, at the highest quality that stays within 1 MiB.
    static func pictureOfOneMebibyte(width: Int, height: Int) -> Data? {
        var quality = 1.0
        while quality > 0.05 {
            if let data = picture(width: width, height: height, quality: quality), data.count <= 1_024 * 1_024 {
                return data
            }
            quality -= 0.02
        }
        return nil
    }

    /// A JPEG with a gradient and many coloured shapes, so that it compresses as a photo does.
    static func picture(width: Int, height: Int, quality: Double) -> Data? {
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        let renderer = UIGraphicsImageRenderer(size: CGSize(width: width, height: height), format: format)
        var generator = SeededGenerator(seed: 42)
        let image = renderer.image { context in
            UIColor(red: 0.2, green: 0.4, blue: 0.7, alpha: 1).setFill()
            context.fill(CGRect(x: 0, y: 0, width: width, height: height))
            for _ in 0 ..< 3_000 {
                UIColor(
                    red: .random(in: 0 ... 1, using: &generator),
                    green: .random(in: 0 ... 1, using: &generator),
                    blue: .random(in: 0 ... 1, using: &generator),
                    alpha: 0.6
                ).setFill()
                let side = CGFloat.random(in: 2 ... max(2, CGFloat(min(width, height)) / 4), using: &generator)
                let origin = CGPoint(
                    x: .random(in: 0 ... CGFloat(width), using: &generator),
                    y: .random(in: 0 ... CGFloat(height), using: &generator)
                )
                context.cgContext.fillEllipse(in: CGRect(origin: origin, size: CGSize(width: side, height: side)))
            }
        }
        return image.jpegData(compressionQuality: quality)
    }

    static func directorySize(_ url: URL) throws -> Int {
        try FileManager.default.contentsOfDirectory(at: url, includingPropertiesForKeys: [.fileSizeKey])
            .reduce(0) { $0 + ((try? $1.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0) }
    }

    static func milliseconds(since start: ContinuousClock.Instant) -> Double {
        let elapsed = ContinuousClock.now - start
        return Double(elapsed.components.seconds) * 1_000 + Double(elapsed.components.attoseconds) / 1e15
    }

    static func median(_ values: [Double]) -> String {
        let sorted = values.sorted()
        return format(sorted.isEmpty ? 0 : sorted[sorted.count / 2])
    }

    static func format(_ value: Double) -> String {
        String(format: "%.2f", value)
    }
}

private struct NoPreimages: TrUAPIPreimageLookuping {
    func lookup(key _: Data) async -> Data? { nil }
}

/// A small fixed generator, so the picture is the same in each run.
private struct SeededGenerator: RandomNumberGenerator {
    private var state: UInt64

    init(seed: UInt64) {
        state = seed
    }

    mutating func next() -> UInt64 {
        state = state &* 6_364_136_223_846_793_005 &+ 1_442_695_040_888_963_407
        return state
    }
}
