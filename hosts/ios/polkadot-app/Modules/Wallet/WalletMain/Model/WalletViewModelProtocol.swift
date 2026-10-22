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

protocol WalletViewModelProtocol: AnyObject, Observation.Observable {
    var identityDetailsViewModel: IdentityDetailsViewModel { get set }
    var assetDetailsViewModel: AssetDetailsViewModel { get set }
    var expandedSection: WalletExpandedSection { get set }
    var isCollectiblesAvailable: Bool { get set }
    var pocketCards: [PocketCardViewModel] { get set }
    var pocketCardFrames: [PocketCardKey: CGRect] { get set }
    var openPocketCard: PocketCardKey? { get set }
    var openPocketCardOffset: CGSize { get set }
    var isPocketCleared: Bool { get set }

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
    /// Where each card is in the window, for its screen to fly from. Read on
    /// demand, so moving a card never redraws the Pocket.
    @ObservationIgnored var pocketCardFrames: [PocketCardKey: CGRect] = [:]
    var openPocketCard: PocketCardKey?
    /// How far the open card is drawn from its slot. The Pocket draws the card
    /// while it flies, so it keeps its place in the stack.
    var openPocketCardOffset: CGSize = .zero
    /// Whether the rest of the Pocket has cleared away for the open card.
    var isPocketCleared = false

    var onUsername: (() -> Void)?
    var onBalance: (() -> Void)?
    var onCollapse: (() -> Void)?
    var onCollectibles: (() -> Void)?
    var onViewCollectibles: (() -> Void)?
    var onOpenPocketCard: ((PocketCardViewModel) -> Void)?
    var onRemovePocketCard: ((PocketCardViewModel) -> Void)?
}
