import Foundation
import Products
import TrUAPIHost

/// Decides where a product runs from when it opens, so its page and its Worker
/// never disagree: a product the app ships runs from the bundle, everything
/// else from dotNS.
struct ProductSources: Sendable {
    enum Source {
        /// Shipped in the app and read from its files, with no dotNS read.
        /// `worker` is nil when it ships none or its manifest is rejected.
        case bundled(BundledProduct, worker: ProductExecutable.Worker?)
        /// Published on dotNS.
        case published
    }

    private let bundledProducts: BundledProducts
    private let screening: PocketCardScreening
    private let logger: LoggerProtocol
    private let debugWidgetURL: @Sendable (PocketCardKey, String) -> URL?

    init(
        bundledProducts: BundledProducts,
        screening: PocketCardScreening = .core,
        logger: LoggerProtocol = Logger.shared,
        debugWidgetURL: @escaping @Sendable (PocketCardKey, String) -> URL? = { _, _ in nil }
    ) {
        self.bundledProducts = bundledProducts
        self.screening = screening
        self.logger = logger
        self.debugWidgetURL = debugWidgetURL
    }

    static let app = ProductSources(bundledProducts: .app, debugWidgetURL: { key, pageQuery in
        #if DEBUG
            DebugPocketCards().widgetURL(for: key, pageQuery: pageQuery)
        #else
            nil
        #endif
    })

    func source(of productId: ProductId) -> Source {
        guard let bundled = bundledProducts.product(productId) else { return .published }

        return .bundled(bundled, worker: bundled.worker(screening: screening, logger: logger))
    }

    /// What an expanded card loads: a widget address typed in debug settings,
    /// then the App the product ships, then the product on dotNS.
    func cardContent(for key: PocketCardKey, pageQuery: String) -> SPAContentSource {
        if let widgetURL = debugWidgetURL(key, pageQuery) {
            return .directURL(widgetURL)
        }

        return source(of: key.productId).pageContent
    }
}

extension ProductSources.Source {
    /// Where its page loads from. A bundled product without an App has no
    /// page of its own to serve, so it falls back to dotNS.
    var pageContent: SPAContentSource {
        switch self {
        case let .bundled(product, _) where product.hasApp():
            .bundled(product.appDirectory)
        case .bundled,
             .published:
            .dotNs
        }
    }
}
