import Observation
import Foundation
import PolkadotUI

enum WalletExpandedSection: Equatable {
    case none
    case identityDetails
    case assetDetails

    var isIdentityShown: Bool {
        showsSection(.identityDetails)
    }

    var isAssetDetailsShown: Bool {
        showsSection(.assetDetails)
    }

    func showsSection(_ section: Self) -> Bool {
        self == .none || self == section
    }
}

protocol WalletViewModelProtocol: Observation.Observable {
    var identityDetailsViewModel: IdentityDetailsViewModel { get set }
    var assetDetailsViewModel: AssetDetailsViewModel { get set }
    var expandedSection: WalletExpandedSection { get set }

    var onUsername: (() -> Void)? { get set }
    var onBalance: (() -> Void)? { get set }
    var onCollapse: (() -> Void)? { get set }
}

@Observable
class WalletViewModel: WalletViewModelProtocol {
    var identityDetailsViewModel: IdentityDetailsViewModel = .init()
    var assetDetailsViewModel: AssetDetailsViewModel = .init()
    var expandedSection: WalletExpandedSection = .none

    var onUsername: (() -> Void)?
    var onBalance: (() -> Void)?
    var onCollapse: (() -> Void)?
}
