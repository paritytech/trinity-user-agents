import Foundation

/// Products that never see a permission or payment prompt. Builds without an Apps settings screen
/// cannot let the user review a grant, so the shipped funding product is allowlisted there.
enum ProductAutoAllowList {
    static func labels(fundingProvider: FundingDomainProviding) -> Set<String> {
        #if FEATURE_PRODUCTS
            []
        #else
            fundingProvider.fundingLabels()
        #endif
    }
}
