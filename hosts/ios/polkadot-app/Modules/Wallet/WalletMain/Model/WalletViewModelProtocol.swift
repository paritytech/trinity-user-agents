import Observation
import Foundation
import PolkadotUI

enum WalletExpandedSection: Equatable {
    case none
    case identityDetails
    case assetDetails
    case collectiblesDetails

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
    var isCollectiblesAvailable: Bool { get set }
    var pocketCards: [PocketCardViewModel] { get set }

    var onUsername: (() -> Void)? { get set }
    var onBalance: (() -> Void)? { get set }
    var onCollapse: (() -> Void)? { get set }
    var onCollectibles: (() -> Void)? { get set }
    var onViewCollectibles: (() -> Void)? { get set }
    var onOpenPocketCard: ((PocketCardViewModel) -> Void)? { get set }
    var onRemovePocketCard: ((PocketCardViewModel) -> Void)? { get set }
}

@Observable
class WalletViewModel: WalletViewModelProtocol {
    var identityDetailsViewModel: IdentityDetailsViewModel = .init()
    var assetDetailsViewModel: AssetDetailsViewModel = .init()
    var expandedSection: WalletExpandedSection = .none
    var isCollectiblesAvailable: Bool = false
    var pocketCards: [PocketCardViewModel] = []

    var onUsername: (() -> Void)?
    var onBalance: (() -> Void)?
    var onCollapse: (() -> Void)?
    var onCollectibles: (() -> Void)?
    var onViewCollectibles: (() -> Void)?
    var onOpenPocketCard: ((PocketCardViewModel) -> Void)?
    var onRemovePocketCard: ((PocketCardViewModel) -> Void)?
}
