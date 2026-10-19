import Foundation
import Kingfisher
import PolkadotUI
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

    /// A face can show one picture twice, and a load that never returns
    /// leaves that cell on its spinner. The server answers late, so both
    /// loads of an address join one download, where Kingfisher used to drop one.
    @Test(.timeLimit(.minutes(1)))
    func overlappingLoadsOfOneAddressBothReturn() async {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [LatePictureServer.self]
        let downloader = ImageDownloader(name: "overlapping-loads")
        downloader.sessionConfiguration = configuration
        let images = WidgetImageResolver.cached(
            logger: MockLogger(),
            options: { _ in [.downloader(downloader), .forceRefresh, .cacheMemoryOnly] },
            locate: { source in
                guard case let .archive(path) = source else { return nil }
                return URL(string: "https://pictures.test/\(path)")
            }
        )

        let loaded = await withTaskGroup(of: UIImage?.self) { group in
            for picture in 0 ..< 10 {
                for _ in 0 ..< 2 {
                    group.addTask { await images(.archive(path: "\(picture).png")) }
                }
            }
            return await group.reduce(into: []) { $0.append($1) }
        }

        #expect(loaded.count == 20)
        #expect(loaded.allSatisfy { $0 != nil })
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
    let format = UIGraphicsImageRendererFormat()
    format.scale = 1
    let png = UIGraphicsImageRenderer(size: size, format: format).pngData { context in
        UIColor.red.setFill()
        context.fill(CGRect(origin: .zero, size: size))
    }
    let file = URL.temporaryDirectory.appending(path: "test-picture-\(UUID().uuidString).png")
    try png.write(to: file)
    return file
}

/// Answers every request with a small picture after a pause long enough for
/// overlapping loads of one address to meet in the downloader.
private final class LatePictureServer: URLProtocol {
    private static let picture = UIGraphicsImageRenderer(size: CGSize(width: 3, height: 2)).pngData { context in
        UIColor.red.setFill()
        context.fill(CGRect(x: 0, y: 0, width: 3, height: 2))
    }

    override class func canInit(with _: URLRequest) -> Bool { true }

    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        DispatchQueue.global().asyncAfter(deadline: .now() + .milliseconds(200)) { [self] in
            guard let url = request.url,
                  let response = HTTPURLResponse(url: url, statusCode: 200, httpVersion: nil, headerFields: nil)
            else { return }
            client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
            client?.urlProtocol(self, didLoad: Self.picture)
            client?.urlProtocolDidFinishLoading(self)
        }
    }

    override func stopLoading() {}
}
