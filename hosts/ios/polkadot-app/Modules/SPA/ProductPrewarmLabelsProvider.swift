import Foundation

protocol ProductPrewarmLabelsProviding {
    func labels() async -> [String]
}

struct ProductPrewarmLabelsProvider {
    let flowStateProvider: SPAFlowStateProviding
}

extension ProductPrewarmLabelsProvider: ProductPrewarmLabelsProviding {
    func labels() async -> [String] {
        #if FEATURE_PRODUCTS
            let staticProducts = [AppConfig.DotNs.dotNsBrowse]
        #else
            let staticProducts: [String] = []
        #endif

        let fundingProvider = FundingDomainProvider(
            hostProvider: flowStateProvider.flowState().hostProvider
        )

        let fundingPages = await [
            try? fundingProvider.fundingPage(),
            try? fundingProvider.offrampPage()
        ]

        return staticProducts + fundingPages.compactMap { $0?.host.name }
    }
}
