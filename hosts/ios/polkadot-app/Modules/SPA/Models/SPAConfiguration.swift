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

/// Where an SPA screen lives, which decides its chrome.
enum SPAPresentation {
    /// Pushed onto a navigation stack.
    case pushed
    /// Mounted as a browser tab, with the browser chrome.
    case browserTab
    /// A one-shot sheet over the tabs, without browser chrome.
    case sheet
}

struct SPAConfiguration {
    let title: String?
    let isRootScreen: Bool
    let showMoreButton: Bool
    let page: ProductPage
    let contentSource: SPAContentSource
    let presentation: SPAPresentation
    let browserTabId: UUID?
    /// How a Pocket card's page reaches the face above it; nil for a page
    /// shown anywhere else.
    let cardFace: (any ExpandedCardFaceShowing)?

    /// Which of the product's executables this screen serves. A Pocket card
    /// opens the widget, everything else the app.
    var executable: ExecutableKind {
        cardFace == nil ? .app : .widget
    }

    init(
        title: String?,
        isRootScreen: Bool,
        showMoreButton: Bool,
        page: ProductPage,
        contentSource: SPAContentSource = .dotNs,
        presentation: SPAPresentation = .pushed,
        browserTabId: UUID? = nil,
        cardFace: (any ExpandedCardFaceShowing)? = nil
    ) {
        self.title = title
        self.isRootScreen = isRootScreen
        self.showMoreButton = showMoreButton
        self.page = page
        self.contentSource = contentSource
        self.presentation = presentation
        self.browserTabId = browserTabId
        self.cardFace = cardFace
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
