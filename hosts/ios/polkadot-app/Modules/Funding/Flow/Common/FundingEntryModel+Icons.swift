import Foundation
import TrUAPIHost
import UIKit

/// One provider's icon for a network or token.
struct FundingIconKey: Hashable {
    let kind: FundingIconKind
    let id: String
    let provider: ProductId
}

extension FundingEntryModel {
    /// The network's icon, from the provider it is drawn for, once read.
    func icon(network id: String) -> UIImage? {
        iconKey(network: id).flatMap { icons[$0] }
    }

    /// The token's icon, from the provider it is drawn for, once read.
    func icon(token symbol: String) -> UIImage? {
        iconKey(token: symbol).flatMap { icons[$0] }
    }

    /// Every icon the flow's network and token marks draw now.
    var iconKeys: [FundingIconKey] {
        let networkIds = networks.map(\.id) + [network].compactMap(\.self)
        return networkIds.compactMap { iconKey(network: $0) } + tokens.compactMap { iconKey(token: $0.symbol) }
    }
}

private extension FundingEntryModel {
    func iconKey(network id: String) -> FundingIconKey? {
        iconProvider { $0.offersNetwork(id) }.map { FundingIconKey(kind: .network, id: id, provider: $0) }
    }

    func iconKey(token symbol: String) -> FundingIconKey? {
        iconProvider { $0.offersToken(symbol, on: network) }
            .map { FundingIconKey(kind: .token, id: symbol, provider: $0) }
    }

    /// The provider a mark is drawn for: the one the session or the user
    /// chose when its routes offer what the mark shows, else the first
    /// listed provider whose routes do. Never one that does not offer it.
    func iconProvider(offering offers: (FundingRoute) -> Bool) -> ProductId? {
        let candidates = entry.options?.candidates ?? []
        let offering = candidates.filter { $0.routes.contains(where: offers) }.map(\.providerId)
        if let chosen = session?.providerId ?? draft.chosenProviderId, offering.contains(chosen) {
            return chosen
        }
        return offering.first
    }
}

private extension FundingRoute {
    func offersNetwork(_ id: String) -> Bool {
        rail == .crypto && (networks ?? []).contains { $0.caseInsensitiveCompare(id) == .orderedSame }
    }

    /// The token on `network`, or on any network the route names before one
    /// is picked.
    func offersToken(_ symbol: String, on network: String?) -> Bool {
        guard rail == .crypto, assets.contains(where: { $0.caseInsensitiveCompare(symbol) == .orderedSame })
        else { return false }

        guard let network else { return true }
        return offersNetwork(network)
    }
}
