import Testing
@testable import Products

struct ProductContentTypeTests {
    @Test func keepsTheTypeTheCoreGave() {
        #expect(ProductContentType.of(path: "/app/index.html", known: "text/html; charset=utf-8")
            == "text/html; charset=utf-8")
    }

    /// The core types only common web extensions and answers the rest as
    /// octet-stream, which breaks media and documents in the WebView.
    @Test(arguments: [
        ("/app/clip.mp4", "video/mp4"),
        ("/app/track.mp3", "audio/mpeg"),
        ("/app/doc.pdf", "application/pdf")
    ])
    func fallsBackToTheSystemTypeWhenTheCoreHasNone(path: String, expected: String) {
        #expect(ProductContentType.of(path: path, known: ProductContentType.unknown) == expected)
        #expect(ProductContentType.of(path: path, known: "") == expected)
        #expect(ProductContentType.of(path: path) == expected)
    }

    @Test func unknownExtensionStaysOctetStream() {
        #expect(ProductContentType.of(path: "/app/blob.zzzunknown", known: "") == ProductContentType.unknown)
        #expect(ProductContentType.of(path: "/app/LICENSE") == ProductContentType.unknown)
    }
}
