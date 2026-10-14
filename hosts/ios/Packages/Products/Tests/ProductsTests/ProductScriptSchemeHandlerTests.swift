import Foundation
import Testing
@testable import Products

struct ProductScriptSchemeHandlerTests {
    private static let entry = "index.html"

    @Test func rootServesTheEntry() {
        let handler = makeHandler(files: [Self.entry: "<root/>"])

        #expect(handler.resolveContent(for: "")?.data == data("<root/>"))
    }

    /// The deep-link regression: a client-side route is not a file, so the handler must fall back
    /// to the entry (history-API fallback) instead of 404ing before the SPA can boot.
    @Test func extensionlessRouteFallsBackToTheEntry() {
        let handler = makeHandler(files: [Self.entry: "<root/>"])

        let resolved = handler.resolveContent(for: "navigation")

        #expect(resolved?.relativePath == Self.entry)
        #expect(resolved?.data == data("<root/>"))
    }

    @Test func exactAssetIsServedVerbatim() {
        let handler = makeHandler(files: [Self.entry: "<root/>", "assets/app.js": "console.log(1)"])

        let resolved = handler.resolveContent(for: "assets/app.js")

        #expect(resolved?.relativePath == "assets/app.js")
        #expect(resolved?.data == data("console.log(1)"))
    }

    /// A missing asset must stay a 404: an extensioned miss is a broken resource, not a route,
    /// so serving HTML in its place would hide the failure.
    @Test func missingAssetIsNotFallenBack() {
        let handler = makeHandler(files: [Self.entry: "<root/>"])

        #expect(handler.resolveContent(for: "assets/missing.js") == nil)
    }

    /// A product that ships a nested index still gets it, rather than the root entry.
    @Test func nestedIndexIsPreferredOverTheRootEntry() {
        let handler = makeHandler(files: [Self.entry: "<root/>", "docs/index.html": "<docs/>"])

        let resolved = handler.resolveContent(for: "docs")

        #expect(resolved?.relativePath == "docs/index.html")
        #expect(resolved?.data == data("<docs/>"))
    }
}

private extension ProductScriptSchemeHandlerTests {
    func makeHandler(files: [String: String]) -> ProductScriptSchemeHandler {
        ProductScriptSchemeHandler(
            productId: "browse.dot",
            entryRelativePath: Self.entry,
            productFileProvider: StubProductFileProvider(files: files.mapValues(data))
        )
    }

    func data(_ string: String) -> Data {
        Data(string.utf8)
    }
}

private struct StubProductFileProvider: ProductFileProviding {
    let files: [String: Data]

    func load(for _: ProductId, relativePath: String) -> Data? {
        files[relativePath]
    }
}
