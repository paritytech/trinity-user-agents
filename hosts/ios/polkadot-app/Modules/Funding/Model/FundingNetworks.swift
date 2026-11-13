import SwiftUI
import TrUAPIHost

/// How a crypto network id from a provider's routes is shown. The ids are the
/// lowercase names the core uses (`polkadot` is Asset Hub), and the core names
/// them. The icons are getcash's, with the app's own Polkadot logo; an id
/// without one shows its first letter.
struct FundingNetwork: Identifiable, Hashable {
    let id: String

    var name: String {
        fundingNetworkName(id: id)
    }

    var logo: ImageResource? {
        Self.logos[id]
    }

    var monogram: String {
        String(name.prefix(1)).uppercased()
    }
}

/// How a token symbol is shown in the token list. A network's native token
/// takes the network's icon; USDC, USDT and DOT take the app's own logos.
struct FundingToken: Identifiable, Hashable {
    let symbol: String

    var id: String { symbol }

    var logo: ImageResource? {
        Self.logos[symbol.uppercased()]
    }

    var monogram: String {
        String(symbol.prefix(1)).uppercased()
    }
}

private extension FundingNetwork {
    static let logos: [String: ImageResource] = [
        "polkadot": .iconDot,
        "bitcoin": .iconBtc,
        "ethereum": .iconEth,
        "solana": .iconSol,
        "tron": .iconTrx
    ]
}

private extension FundingToken {
    static let logos: [String: ImageResource] = [
        "DOT": .iconDot,
        "BTC": .iconBtc,
        "ETH": .iconEth,
        "SOL": .iconSol,
        "TRX": .iconTrx,
        "USDC": .iconUsdc,
        "USDT": .iconUsdt,
        "DOTUSD": .iconDotusd
    ]
}
