import Foundation
import Products

enum SPAContentSource {
    /// Resolve the dotNs product and serve it via the polkadot:// scheme handler.
    case dotNs
    /// Debug: load the URL as-is, skipping resolution. Rust runtime only.
    case directURL(URL)
    #if DEBUG
        /// Debug: a product served from a development server on the developer's machine, loaded from
        /// `url` and run under the identifier the core gave it. Rust runtime only.
        case devServer(url: URL, productId: ProductId)
    #endif
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
    /// Whether the screen carries the browser chrome: the floating top pills, chrome collapse and
    /// minimize. A product presented as a modal has none of them.
    let usesBrowserChrome: Bool

    init(
        title: String?,
        isRootScreen: Bool,
        showMoreButton: Bool,
        page: ProductPage,
        contentSource: SPAContentSource = .dotNs,
        isBrowserTab: Bool = false,
        browserTabId: UUID? = nil,
        executable: ExecutableKind = .app,
        usesBrowserChrome: Bool = true
    ) {
        self.title = title
        self.isRootScreen = isRootScreen
        self.showMoreButton = showMoreButton
        self.page = page
        self.contentSource = contentSource
        self.isBrowserTab = isBrowserTab
        self.browserTabId = browserTabId
        self.executable = executable
        self.usesBrowserChrome = usesBrowserChrome
    }
}

extension SPAConfiguration {
    /// Identifier the product runs under, which storage, permissions and account derivation key off.
    var productId: ProductId {
        switch contentSource {
        case .dotNs,
             .directURL:
            page.host.toDotDomain()
        #if DEBUG
            case let .devServer(_, productId):
                productId
        #endif
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
