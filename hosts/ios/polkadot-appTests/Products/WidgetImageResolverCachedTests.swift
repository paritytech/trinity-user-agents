import Foundation
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
