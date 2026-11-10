import Foundation
import Products

/// The two funding-product entry points; each opens the page remote config names.
enum RampAction: Sendable {
    case topUp
    case withdraw

    func resolvePage(using provider: FundingDomainProviding) async throws -> ProductPage {
        switch self {
        case .topUp: try await provider.fundingPage()
        case .withdraw: try await provider.offrampPage()
        }
    }
}
