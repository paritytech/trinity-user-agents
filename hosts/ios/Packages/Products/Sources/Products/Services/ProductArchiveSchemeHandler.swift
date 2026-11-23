import Foundation
import TrUAPIHost
import WebKit

/// Serves a published App's archive on disk under the product's own origin.
/// The core lists the files that may answer each request, in order, with
/// their MIME types; the first regular file on disk is served, with the
/// system's type for extensions the core leaves untyped, and a request
/// nothing answers gets a 404.
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

    /// The first file the core lists for the request that is a regular file
    /// on disk, so a directory is skipped for its `index.html`.
    func file(at requestPath: String, isMainFrame: Bool) -> (data: Data, mime: String)? {
        for file in archiveFiles(root: root.path, requestPath: requestPath, isMainFrame: isMainFrame) {
            guard let data = regularFile(at: file.path) else { continue }
            return (data, ProductContentType.of(path: file.path, known: file.mime))
        }
        return nil
    }

    private func regularFile(at path: String) -> Data? {
        var isDirectory: ObjCBool = false
        guard fileManager.fileExists(atPath: path, isDirectory: &isDirectory), !isDirectory.boolValue else {
            return nil
        }
        return fileManager.contents(atPath: path)
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
