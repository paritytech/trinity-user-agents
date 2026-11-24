import Foundation
import TrUAPIHost

/// How a crypto network id from a provider's routes is shown. The ids are the
/// lowercase names the core uses (`polkadot` is Asset Hub), and the core names
/// them. Each shows its first letter.
struct FundingNetwork: Identifiable, Hashable {
    let id: String

    var name: String {
        fundingNetworkName(id: id)
    }

    var monogram: String {
        String(name.prefix(1)).uppercased()
    }
}

/// How a token symbol is shown in the token list: its first letter.
struct FundingToken: Identifiable, Hashable {
    let symbol: String

    var id: String { symbol }

    var monogram: String {
        String(symbol.prefix(1)).uppercased()
    }
}
