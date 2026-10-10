import Foundation
import Products

enum SPAContentSource {
    /// Resolve the dotNs product and serve it via the polkadot:// scheme handler.
    case dotNs
    /// Debug: load the URL as-is, skipping resolution. Rust runtime only.
    case directURL(URL)
    /// Debug: a product served from a development server on the developer's machine, loaded from
    /// `origin` and run under the core's `localhost[:port]` identifier. Rust runtime only.
    case devServer(origin: URL, productId: ProductId)
}

struct SPAConfiguration {
    let title: String?
    let isRootScreen: Bool
    let showMoreButton: Bool
    let page: ProductPage
    let contentSource: SPAContentSource
    let isBrowserTab: Bool
    let browserTabId: UUID?
    /// Which of the product's executables this screen serves. A Pocket card
    /// opens the widget, everything else the app.
    let executable: ExecutableKind

    init(
        title: String?,
        isRootScreen: Bool,
        showMoreButton: Bool,
        page: ProductPage,
        contentSource: SPAContentSource = .dotNs,
        isBrowserTab: Bool = false,
        browserTabId: UUID? = nil,
        executable: ExecutableKind = .app
    ) {
        self.title = title
        self.isRootScreen = isRootScreen
        self.showMoreButton = showMoreButton
        self.page = page
        self.contentSource = contentSource
        self.isBrowserTab = isBrowserTab
        self.browserTabId = browserTabId
        self.executable = executable
    }
}

extension SPAConfiguration {
    /// Identifier the product runs under, which storage, permissions and account derivation key off.
    var productId: ProductId {
        switch contentSource {
        case .dotNs,
             .directURL:
            page.host.toDotDomain()
        case let .devServer(_, productId):
            productId
        }
    }

    static func browseRoot(host: ProductHost) -> SPAConfiguration {
        SPAConfiguration(
            title: nil,
            isRootScreen: true,
            showMoreButton: false,
            page: ProductPage(host: host)
        )
    }
}
