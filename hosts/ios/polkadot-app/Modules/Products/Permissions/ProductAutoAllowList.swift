import Foundation

/// Products that never see a permission or payment prompt: the funding product the CASH card opens.
enum ProductAutoAllowList {
    static func labels(fundingProvider: FundingDomainProviding) -> Set<String> {
        fundingProvider.fundingLabels()
    }
}
