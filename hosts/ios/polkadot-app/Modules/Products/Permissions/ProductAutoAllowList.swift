import Foundation

/// Labels of the funding products, which skip the permission and payment prompts. Funding products
/// open modally as one-off flows and are not listed for review in Apps, so there is no grant the
/// user could review first. Payment approvals read this same list.
enum ProductAutoAllowList {
    static func labels(fundingProvider: FundingDomainProviding) -> Set<String> {
        fundingProvider.fundingLabels()
    }
}
