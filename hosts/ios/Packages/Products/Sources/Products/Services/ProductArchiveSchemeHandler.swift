import Foundation
import TrUAPIHost
import WebKit

/// Serves an App's archive on disk, bundled or published, under the
/// product's own origin. The core picks the file that answers each request
/// and its MIME type; a request nothing answers gets a 404.
public final class ProductArchiveSchemeHandler: NSObject {
    private let productId: ProductId
    private let root: URL
    private let fileManager: FileManager

    public init(productId: ProductId, root: URL, fileManager: FileManager) {
        self.productId = productId
        self.root = root
        self.fileManager = fileManager
    }

    public func getBaseUrl() -> URL? {
        var components = URLComponents()
        components.scheme = productContentScheme()
        components.host = productId.rawValue
        components.path = "/"
        return components.url
    }

    func file(at requestPath: String, isMainFrame: Bool) -> (data: Data, mime: String)? {
        guard let file = archiveFile(
            root: root.path,
            requestPath: requestPath,
            isMainFrame: isMainFrame,
            reader: FileBundleReader()
        ),
            let data = fileManager.contents(atPath: file.path)
        else { return nil }

        return (data, file.mime)
    }
}

extension ProductArchiveSchemeHandler: WKURLSchemeHandler {
    public func webView(_: WKWebView, start urlSchemeTask: any WKURLSchemeTask) {
        guard let url = urlSchemeTask.request.url else {
            urlSchemeTask.didFailWithError(URLError(.badURL))
            return
        }

        let isMainFrame = urlSchemeTask.request.mainDocumentURL == url
        let file = file(at: url.path, isMainFrame: isMainFrame)
        let data = file?.data ?? Data()
        let headers = file.map { ["Content-Type": $0.mime, "Content-Length": "\(data.count)"] } ?? [:]
        guard let response = HTTPURLResponse(
            url: url,
            statusCode: file == nil ? 404 : 200,
            httpVersion: "HTTP/1.1",
            headerFields: headers
        ) else {
            urlSchemeTask.didFailWithError(URLError(.cannotParseResponse))
            return
        }

        urlSchemeTask.didReceive(response)
        urlSchemeTask.didReceive(data)
        urlSchemeTask.didFinish()
    }

    public func webView(_: WKWebView, stop _: any WKURLSchemeTask) {}
}
