import Foundation
import PolkadotUI
import SVGKit
import Testing
import UIKit
@testable import polkadot_app

/// A card image that cannot be shown leaves its space empty, so the log is
/// the only place a broken source shows up.
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
