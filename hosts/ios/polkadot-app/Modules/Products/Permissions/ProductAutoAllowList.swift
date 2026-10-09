import Foundation

/// Products that never see a permission or payment prompt. The shipped funding product is
/// allowlisted: the user cannot review a grant it would otherwise ask for.
enum ProductAutoAllowList {
    static func labels(fundingProvider: FundingDomainProviding) -> Set<String> {
        fundingProvider.fundingLabels()
    }
}
