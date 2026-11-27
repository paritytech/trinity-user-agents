import Observation
import Foundation
import PolkadotUI

/// How the Collectibles peek shows on the Pocket before its screen opens.
enum CollectiblesPeek: Equatable {
    case resting
    case colouring
    case lifted
}

protocol WalletViewModelProtocol: AnyObject, Observation.Observable {
    var assetDetailsViewModel: AssetDetailsViewModel { get set }
    var isCollectiblesAvailable: Bool { get set }
    var pocketCards: [PocketCardViewModel] { get set }
    var pocketCardFrames: [PocketCardKey: CGRect] { get set }
    var openItem: PocketItem? { get set }
    var openPocketCardOffset: CGSize { get set }
    var pocketClearance: CGFloat { get set }
    var collectiblesPeek: CollectiblesPeek { get set }
    var balanceCardFrame: CGRect { get set }
    var collectiblesFrame: CGRect { get set }

    var onBalance: (() -> Void)? { get set }
    var onCollectibles: (() -> Void)? { get set }
    var onCollectiblesPress: ((Bool) -> Void)? { get set }
    var onOpenPocketCard: ((PocketCardViewModel) -> Void)? { get set }
    var onRemovePocketCard: ((PocketCardViewModel) -> Void)? { get set }
}

extension WalletViewModelProtocol {
    var isPocketCleared: Bool {
        pocketClearance > 0
    }
}

@Observable
class WalletViewModel: WalletViewModelProtocol {
    var assetDetailsViewModel: AssetDetailsViewModel = .init()
    var isCollectiblesAvailable: Bool = false
    var pocketCards: [PocketCardViewModel] = []
    /// Where each card is in the window, for its screen to fly from. Read on
    /// demand, so moving a card never redraws the Pocket.
    @ObservationIgnored var pocketCardFrames: [PocketCardKey: CGRect] = [:]
    var openItem: PocketItem?
    /// How far the open card is drawn from its slot. The Pocket draws the card
    /// while it flies, so it keeps its place in the stack.
    var openPocketCardOffset: CGSize = .zero
    /// How far the rest of the Pocket has cleared away for the open card: 0 at rest, 1 cleared.
    var pocketClearance: CGFloat = 0
    var collectiblesPeek: CollectiblesPeek = .resting
    /// Where the Balance card is in the window, for its screen to fly from. Read on demand, so moving it never
    /// redraws the Pocket.
    @ObservationIgnored var balanceCardFrame: CGRect = .zero
    /// Where the Collectibles peek is in the window, for its book to lift from. Read on demand, so moving
    /// it never redraws the Pocket.
    @ObservationIgnored var collectiblesFrame: CGRect = .zero

    var onBalance: (() -> Void)?
    var onCollectibles: (() -> Void)?
    var onCollectiblesPress: ((Bool) -> Void)?
    var onOpenPocketCard: ((PocketCardViewModel) -> Void)?
    var onRemovePocketCard: ((PocketCardViewModel) -> Void)?
}
