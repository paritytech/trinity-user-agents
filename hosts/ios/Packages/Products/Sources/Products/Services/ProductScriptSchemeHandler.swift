import Foundation
import WebKit
import UniformTypeIdentifiers

enum ProductScriptSchemeHandlerError: Error {
    case invalidUrl
}

public final class ProductScriptSchemeHandler: NSObject {
    public static let scheme = "polkadot"

    private let productId: ProductId
    private let entryRelativePath: String
    private let productFileProvider: ProductFileProviding

    public init(
        productId: ProductId,
        entryRelativePath: String,
        productFileProvider: ProductFileProviding
    ) {
        self.productId = productId
        self.entryRelativePath = entryRelativePath
        self.productFileProvider = productFileProvider
    }

    public func getBaseUrl() -> URL? {
        var components = URLComponents()
        components.scheme = Self.scheme
        components.host = productId
        components.path = "/"
        return components.url
    }

    public func getProductUrl() -> URL? {
        getBaseUrl()?.appendingPathComponent(entryRelativePath)
    }
}

extension ProductScriptSchemeHandler: WKURLSchemeHandler {
    static let defaultMimeType: String = "application/octet-stream"

    public func webView(_: WKWebView, start urlSchemeTask: any WKURLSchemeTask) {
        guard let url = urlSchemeTask.request.url else {
            urlSchemeTask.didFailWithError(ProductScriptSchemeHandlerError.invalidUrl)
            return
        }

        let path = url.path
        let requestedPath = path.hasPrefix("/") ? String(path.dropFirst()) : path

        guard let resolved = resolveContent(for: requestedPath) else {
            urlSchemeTask.didFailWithError(URLError(.fileDoesNotExist))
            return
        }

        let pathExtension = (resolved.relativePath as NSString).pathExtension
        let contentType = UTType(filenameExtension: pathExtension)?.preferredMIMEType ?? Self.defaultMimeType
        let data = resolved.data

        let response = HTTPURLResponse(
            url: url,
            statusCode: 200,
            httpVersion: "HTTP/1.1",
            headerFields: [
                "Content-Type": contentType,
                "Content-Length": "\(data.count)"
            ]
        )!

        urlSchemeTask.didReceive(response)
        urlSchemeTask.didReceive(data)
        urlSchemeTask.didFinish()
    }

    public func webView(_: WKWebView, stop _: any WKURLSchemeTask) {
        // No cancellation needed for synchronous file reads
    }

    func resolveContent(for requestedPath: String) -> (relativePath: String, data: Data)? {
        guard !requestedPath.isEmpty else {
            return loadContent(at: entryRelativePath)
        }

        if let exact = loadContent(at: requestedPath) {
            return exact
        }

        // A path with an extension is an asset request; a miss there is a real 404, not a route.
        guard !requestedPath.contains(".") else {
            return nil
        }

        // Extensionless miss = a client-side route. Try a nested index, then fall back to the root
        // entry so the SPA router can resolve deep links (history-API fallback).
        let directory = requestedPath.hasSuffix("/") ? requestedPath : requestedPath + "/"
        return loadContent(at: directory + entryRelativePath) ?? loadContent(at: entryRelativePath)
    }

    func loadContent(at relativePath: String) -> (relativePath: String, data: Data)? {
        guard let data = productFileProvider.load(for: productId, relativePath: relativePath) else {
            return nil
        }

        return (relativePath, data)
    }
}
