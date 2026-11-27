import Foundation

protocol ProductPrewarmLabelsProviding {
    func labels() async -> [String]
}

struct ProductPrewarmLabelsProvider {}

extension ProductPrewarmLabelsProvider: ProductPrewarmLabelsProviding {
    func labels() async -> [String] {
        // Funding providers are dotNS products the core lists, so only the
        // static products are prewarmed here.
        #if FEATURE_PRODUCTS
            return [AppConfig.DotNs.dotNsBrowse]
        #else
            return []
        #endif
    }
}
