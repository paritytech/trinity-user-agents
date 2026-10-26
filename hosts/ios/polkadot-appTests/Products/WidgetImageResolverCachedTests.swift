import Foundation
import Kingfisher
import PolkadotUI
import SVGKit
import Testing
import UIKit
@testable import polkadot_app

/// A card image that cannot be shown draws a placeholder rather than an
/// error, so the log is the only place a broken source shows up.
struct WidgetImageResolverCachedTests {
    @Test
    func loadsThePictureAtTheLocatedAddress() async throws {
        let file = try writeTestPicture(size: CGSize(width: 3, height: 2))
        defer { try? FileManager.default.removeItem(at: file) }
        let images = WidgetImageResolver.cached(logger: MockLogger()) { _ in file }

        let image = await images(.archive(path: "art/prize.png"))

        #expect(image?.size == CGSize(width: 3, height: 2))
    }

    /// A face can show one picture several times, and a load that never
    /// returns leaves its cell on a spinner. The loads start together and the
    /// server holds its answer, so all of them register for one download.
    @Test(.timeLimit(.minutes(1)))
    func overlappingLoadsOfOneAddressAllReturn() async {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [LatePictureServer.self]
        let downloader = ImageDownloader(name: "overlapping-loads")
        downloader.sessionConfiguration = configuration
        let cache = ImageCache(name: "overlapping-loads")
        let address = URL(string: "https://pictures.test/stamp.png")
        let images = WidgetImageResolver.cached(
            logger: MockLogger(),
            options: { _ in [.downloader(downloader), .targetCache(cache), .forceRefresh, .cacheMemoryOnly] },
            locate: { _ in address }
        )

        let loaded = await withTaskGroup(of: UIImage?.self) { group in
            for _ in 0 ..< 20 {
                group.addTask { await images(.archive(path: "stamp.png")) }
            }
            return await group.reduce(into: []) { $0.append($1) }
        }

        #expect(loaded.allSatisfy { $0 != nil })
    }

    @Test
    func loadsAnSVGAtTheSizeItDeclares() async throws {
        let file = URL.temporaryDirectory.appending(path: "test-picture-\(UUID().uuidString).svg")
        let svg = ##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10" viewBox="0 0 20 10">"## +
            ##"<rect width="20" height="10" fill="#2a6"/><circle cx="10" cy="5" r="4" fill="#d22"/></svg>"##
        try Data(svg.utf8).write(to: file)
        defer { try? FileManager.default.removeItem(at: file) }
        // SVGKit's first parse in a debug build asserts on a device its PPI table
        // does not list, such as the simulator. Release builds skip the assert.
        _ = SVGKImage(data: Data(svg.utf8))
        let images = WidgetImageResolver.cached(logger: MockLogger()) { _ in file }

        let image = await images(.archive(path: "art/logo.svg"))

        #expect(image?.size == CGSize(width: 20, height: 10))
    }

    /// Bytes no decoder reads are a failure, not an empty picture, and are not
    /// kept: the next draw of the same address reads it again.
    @Test
    func logsBytesThatAreNotAPictureAndTriesThemAgain() async throws {
        let file = URL.temporaryDirectory.appending(path: "test-picture-\(UUID().uuidString).png")
        try Data("not a picture".utf8).write(to: file)
        defer { try? FileManager.default.removeItem(at: file) }
        let logger = MockLogger()
        let images = WidgetImageResolver.cached(logger: logger) { _ in file }

        let broken = await images(.archive(path: "art/broken.png"))
        try await logger.waitForError(containing: "failed to load")
        try Data(contentsOf: writeTestPicture(size: CGSize(width: 3, height: 2))).write(to: file)
        let fixed = await images(.archive(path: "art/broken.png"))

        #expect(broken == nil)
        #expect(fixed?.size == CGSize(width: 3, height: 2))
    }

    @Test
    func logsASourceWithNoAddress() async throws {
        let logger = MockLogger()
        let images = WidgetImageResolver.cached(logger: logger) { _ in nil }

        let image = await images(.archive(path: "art/missing.png"))

        #expect(image == nil)
        try await logger.waitForError(containing: "has no address")
    }

    @Test
    func logsAnAddressThatDoesNotLoad() async throws {
        let logger = MockLogger()
        let missing = URL.temporaryDirectory.appending(path: "absent-\(UUID().uuidString).png")
        let images = WidgetImageResolver.cached(logger: logger) { _ in missing }

        let image = await images(.bulletin(cid: "bafymissing"))

        #expect(image == nil)
        try await logger.waitForError(containing: "failed to load")
    }
}

func writeTestPicture(size: CGSize) throws -> URL {
    let file = URL.temporaryDirectory.appending(path: "test-picture-\(UUID().uuidString).png")
    try testPicturePNG(size: size).write(to: file)
    return file
}

func testPicturePNG(size: CGSize) -> Data {
    let format = UIGraphicsImageRendererFormat()
    format.scale = 1
    return UIGraphicsImageRenderer(size: size, format: format).pngData { context in
        UIColor.red.setFill()
        context.fill(CGRect(origin: .zero, size: size))
    }
}

/// Answers every request with a small picture after a pause, so loads that
/// start together are all still waiting when it answers.
private final class LatePictureServer: URLProtocol {
    private static let picture = testPicturePNG(size: CGSize(width: 3, height: 2))

    private var answer: DispatchWorkItem?

    override class func canInit(with _: URLRequest) -> Bool { true }

    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        let answer = DispatchWorkItem { [self] in respond() }
        self.answer = answer
        DispatchQueue.global().asyncAfter(deadline: .now() + .milliseconds(200), execute: answer)
    }

    override func stopLoading() {
        answer?.cancel()
    }

    private func respond() {
        guard let url = request.url,
              let response = HTTPURLResponse(url: url, statusCode: 200, httpVersion: nil, headerFields: nil)
        else {
            client?.urlProtocol(self, didFailWithError: URLError(.badURL))
            return
        }
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: Self.picture)
        client?.urlProtocolDidFinishLoading(self)
    }
}
