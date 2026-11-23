import Foundation
import TrUAPIHost

struct FundingAmountViewModel: Equatable {
    struct Preset: Equatable, Hashable {
        /// The amount as the keypad types it.
        let text: String
        let figure: String
    }

    /// What a withdrawal can spend, once read.
    let available: String?
    let figure: String
    let symbol: String
    let presets: [Preset]
    /// Why the typed amount cannot go ahead.
    let issue: String?
    let minimum: String
}

struct FundingSummaryViewModel: Equatable {
    /// What the user pays for value in, what they receive for value out.
    let headline: String?
    let payout: String?
    let bankRateNote: String
    let problem: String?
}

struct FundingFeesViewModel: Equatable {
    let amount: String?
    let providerFee: String?
    let networkFee: String?
    let total: String?
    let pay: String?
    let rate: String?
}

struct FundingProvidersViewModel: Equatable {
    struct Price: Equatable {
        let provider: String?
        let cash: String?
    }

    let prices: [ProductId: Price]
    let unavailable: [ProductId: String]
}

struct FundingNetworkViewModel: Equatable {
    /// The minimum line of each network closed to the typed amount, by id.
    let minimums: [String: String]
}

struct FundingCountryViewModel: Equatable {
    /// The minimum line of each country a provider showed one for, by code.
    let minimums: [String: String]
}

struct FundingDepositViewModel: Equatable {
    /// What to pay into the deposit the provider named.
    let amount: String?
}

struct FundingProgressViewModel: Equatable {
    struct CancelOffer: Equatable {
        let fee: String
        let refund: String
    }

    let figure: String?
    let symbol: String
    /// The CASH the session is for, with its symbol.
    let amount: String?
    let quotedSend: String?
    let quotedReceive: String?
    let deposit: String?
    let bankRateNote: String
    let cancelOffer: CancelOffer?
    let ended: FundingEndedViewModel?
}

struct FundingEndedViewModel: Equatable {
    /// The CASH that moved, signed once it did.
    let figure: String?
    let symbol: String
    let quoted: String?
    let settled: String?
    let requested: String?
    let refunded: String?
    /// The CASH the session was for, with its symbol.
    let amount: String
}
