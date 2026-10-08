import SwiftUI

/// How a crypto network id from a provider's routes is shown. The ids are the
/// lowercase names the core uses (`polkadot` is Asset Hub); an id this table
/// does not know is shown capitalised.
struct FundingNetwork: Identifiable, Hashable {
    let id: String

    var name: String {
        Self.names[id] ?? id.capitalized
    }

    var tint: Color {
        Self.tints[id] ?? .gray
    }

    var monogram: String {
        String(name.prefix(1)).uppercased()
    }
}

/// How a token symbol is shown in the token list.
struct FundingToken: Identifiable, Hashable {
    let symbol: String

    var id: String { symbol }

    var tint: Color {
        Self.tints[symbol.uppercased()] ?? .gray
    }

    var monogram: String {
        String(symbol.prefix(1)).uppercased()
    }
}

private extension FundingNetwork {
    static let names: [String: String] = [
        "polkadot": "Polkadot",
        "bitcoin": "Bitcoin",
        "ethereum": "Ethereum",
        "tron": "Tron",
        "solana": "Solana"
    ]

    static let tints: [String: Color] = [
        "polkadot": Color(red: 0.9, green: 0.0, blue: 0.48),
        "bitcoin": Color(red: 0.97, green: 0.58, blue: 0.1),
        "ethereum": Color(red: 0.38, green: 0.49, blue: 0.92),
        "tron": Color(red: 0.92, green: 0.0, blue: 0.16),
        "solana": Color(red: 0.6, green: 0.27, blue: 1.0)
    ]
}

private extension FundingToken {
    static let tints: [String: Color] = [
        "USDC": Color(red: 0.16, green: 0.46, blue: 0.79),
        "USDT": Color(red: 0.15, green: 0.63, blue: 0.48)
    ]
}
