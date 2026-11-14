import DesignSystem
import Foundation
import PolkadotUI
import Products
import struct TrUAPIHost.ProductId
import func TrUAPIHost.productContentScheme
import UIKit
import WebKit

// MARK: - Page address and size

/// Where a web room footer loads from, and how tall it may be.
///
/// The page is the product's own: `path` is joined onto the product's origin,
/// `polkadot://<product>/`, which is the origin the host serves the product's
/// pages from, and a result that lands anywhere else is refused.
enum ProductRoomFooterWebPage {
    /// The tallest footer a product may ask for, in points.
    static let maximumHeight: CGFloat = 160
    /// The shortest footer drawn, in points.
    static let minimumHeight: CGFloat = 44

    static func height(_ requested: UInt32) -> CGFloat {
        min(max(CGFloat(requested), minimumHeight), maximumHeight)
    }

    static func height(reported: Double?) -> CGFloat? {
        guard let reported, reported.isFinite, reported > 0 else { return nil }
        return min(max(CGFloat(reported), minimumHeight), maximumHeight)
    }

    static func origin(of productId: ProductId) -> URL? {
        var components = URLComponents()
        components.scheme = productContentScheme()
        components.host = productId.rawValue
        components.path = "/"
        return components.url
    }

    /// The page at `path` on the product's own origin, or nil when `path` would leave it.
    static func url(path: String, productId: ProductId) -> URL? {
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty,
              !trimmed.hasPrefix("/"),
              !trimmed.contains("\\"),
              let origin = origin(of: productId),
              let url = URL(string: trimmed, relativeTo: origin)?.absoluteURL,
              isProductOrigin(url, productId: productId) else {
            return nil
        }
        return url
    }

    static func isProductOrigin(_ url: URL, productId: ProductId) -> Bool {
        url.scheme?.lowercased() == productContentScheme().lowercased() &&
            url.host?.lowercased() == productId.rawValue.lowercased() &&
            url.user == nil && url.password == nil && url.port == nil
    }
}

// MARK: - Page to host

/// One message the footer page posts through `window.webkit.messageHandlers.productFooter`.
enum ProductFooterMessage: Equatable {
    case action(actionId: String, payload: String?)
    case haptic(style: String)
    case overlay(isOn: Bool)
    case ready(height: Double?)

    static let handlerName = "productFooter"
    static let actionIdMaxBytes = 256
    static let payloadMaxBytes = 64 * 1_024

    /// Reads a message posted as an object or as its JSON text. Anything else is dropped.
    init?(body: Any) {
        guard let fields = Self.fields(of: body) else { return nil }

        switch fields["kind"] as? String {
        case "action":
            guard let action = Self.action(from: fields) else { return nil }
            self = action
        case "haptic":
            self = .haptic(style: fields["style"] as? String ?? "success")
        case "overlay":
            guard let isOn = fields["on"] as? Bool else { return nil }
            self = .overlay(isOn: isOn)
        case "ready":
            self = .ready(height: (fields["height"] as? NSNumber)?.doubleValue)
        default:
            return nil
        }
    }
}

private extension ProductFooterMessage {
    /// What a payload reads as: absent, its text, or something that cannot travel.
    enum Payload {
        case absent
        case text(String)
        case invalid
    }

    static func fields(of body: Any) -> [String: Any]? {
        if let object = body as? [String: Any] { return object }
        guard let text = body as? String else { return nil }
        return try? JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any]
    }

    static func action(from fields: [String: Any]) -> ProductFooterMessage? {
        guard let actionId = fields["actionId"] as? String,
              !actionId.isEmpty,
              actionId.utf8.count <= actionIdMaxBytes else {
            return nil
        }
        switch payload(fields["payload"]) {
        case .absent:
            return .action(actionId: actionId, payload: nil)
        case let .text(text) where text.utf8.count <= payloadMaxBytes:
            return .action(actionId: actionId, payload: text)
        case .text,
             .invalid:
            return nil
        }
    }

    /// A text payload travels as it is, any other JSON value as its JSON text.
    static func payload(_ raw: Any?) -> Payload {
        switch raw {
        case nil,
             is NSNull:
            return .absent
        case let text as String:
            return text.isEmpty ? .absent : .text(text)
        case let .some(value):
            guard JSONSerialization.isValidJSONObject(value),
                  let data = try? JSONSerialization.data(withJSONObject: value, options: [.sortedKeys]),
                  let text = String(bytes: data, encoding: .utf8) else {
                return .invalid
            }
            return .text(text)
        }
    }
}

// MARK: - Host to page

/// What the host tells the page through `window.productFooter.receive`.
enum ProductFooterHostEvent: Equatable {
    case reset
    case state(String)

    var json: String {
        switch self {
        case .reset:
            #"{"kind":"reset"}"#
        case let .state(state):
            #"{"kind":"state","state":"\#(state)"}"#
        }
    }

    /// Optional chaining: a page that defines no receiver works without one.
    var script: String {
        "window.productFooter && typeof window.productFooter.receive === 'function' && " +
            "window.productFooter.receive(\(json));"
    }
}

// MARK: - Content

/// Serves a product's own web content to its room footer, the way the product's pages are served.
final class ProductRoomFooterWebContent {
    let productId: ProductId
    private let contentLoader: ProductContentLoading
    private let logger: LoggerProtocol

    init(productId: ProductId, contentLoader: ProductContentLoading, logger: LoggerProtocol) {
        self.productId = productId
        self.contentLoader = contentLoader
        self.logger = logger
    }

    /// A handler for the product scheme that answers once the product's App content is staged.
    @MainActor
    func makeSchemeHandler() -> SchemeHandlerProxy {
        let proxy = SchemeHandlerProxy()
        Task { [productId, contentLoader, logger] in
            do {
                let content = try await contentLoader.open(productId, strategy: .fixed(.app), policy: .attempts(3))
                let handler = ProductScriptSchemeHandler(
                    productId: productId,
                    entryRelativePath: ProductBundle.indexHTML,
                    productFileProvider: DotNsFileProvider(contentURL: content.archive.localURL)
                )
                await proxy.setHandler(handler)
                logger
                    .debug(
                        "[truapi.product] web footer of \(productId.rawValue) serves \(content.archive.localURL.path)"
                    )
            } catch {
                logger.error("[truapi.product] web footer of \(productId.rawValue) has no content: \(error)")
            }
        }
        return proxy
    }
}

// MARK: - Configuration

/// The input slot holding a page of the product's own web content.
struct ProductRoomFooterWebConfiguration: @preconcurrency ChatInputViewConfigurationProtocol {
    let roomId: String
    let path: String
    let height: UInt32
    let makeView: @MainActor () -> UIView

    var activateOnAppear: Bool {
        false
    }

    /// The strip below the slot, down to the screen edge, takes the chat's colour, so no chat
    /// shows under the app's game pill or the home indicator.
    var safeAreaFillColor: UIColor? {
        .bgSurfaceMain
    }

    /// The slider that replaces a docked card, or another page, fades in once the page shows.
    var animatesReplacement: Bool {
        true
    }

    @MainActor
    func makeContentView(for handler: ChatInputHandling?) -> UIView {
        let view = makeView()
        (view as? ProductRoomFooterWebView)?.inputHandler = handler
        return view
    }

    func equalsTo(configuration: any ChatInputViewConfigurationProtocol) -> Bool {
        guard let other = configuration as? ProductRoomFooterWebConfiguration else { return false }

        return roomId == other.roomId && path == other.path && height == other.height
    }

    @MainActor
    func whenContentReady(_ view: UIView, _ ready: @escaping () -> Void) {
        guard let view = view as? ProductRoomFooterWebView else {
            ready()
            return
        }
        view.whenRevealed(ready)
    }
}
