import UIKit

/// What a funding provider is called and looks like, from the root manifest
/// of the provider's product.
struct FundingProviderBrand: Equatable {
    let name: String
    let icon: UIImage?

    static func placeholder(for providerId: String) -> FundingProviderBrand {
        let label = providerId.split(separator: ".").first.map(String.init) ?? providerId
        return FundingProviderBrand(name: label.capitalized, icon: nil)
    }
}

protocol FundingProviderBranding: Sendable {
    func brand(for providerId: String) async -> FundingProviderBrand
}
