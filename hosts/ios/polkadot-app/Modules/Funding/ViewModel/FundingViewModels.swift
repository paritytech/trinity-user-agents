import Foundation
import TrUAPIHost
import UIKit

struct FundingAmountViewModel: Equatable {
    struct Preset: Equatable, Hashable {
        /// The amount as the keypad types it.
        let text: String
        let figure: String
    }

    let title: String
    /// Whether the history opens from the header, and whether a session in
    /// it is still running.
    let history: FundingHistoryClock
    let rail: FundingRail
    let availableRails: Set<FundingRail>
    /// What a withdrawal can spend, once read.
    let available: String?
    /// The amount as the keypad typed it.
    let amountText: String
    let figure: String
    let hasAmount: Bool
    let symbol: String
    let presets: [Preset]
    /// Why the typed amount cannot go ahead.
    let issue: String?
    let minimum: String
    let canContinue: Bool
}

struct FundingSummaryViewModel: Equatable {
    let title: String
    let feesCaption: String
    /// Card and bank show where the user pays from.
    let showsCountry: Bool
    let country: PaymentCountry?
    /// The provider whose quote is on screen.
    let provider: FundingProviderBrand?
    /// What the user pays for value in, what they receive for value out.
    let headline: String?
    let isQuoting: Bool
    let payoutTitle: String
    let payout: String?
    let eta: String?
    /// Shown under the arrival for a bank top-up.
    let bankRateNote: String?
    let problem: String?
    let hasQuote: Bool
    let isStarting: Bool
}

struct FundingFeesViewModel: Equatable {
    struct Breakdown: Equatable {
        let amount: String?
        let providerFee: String?
        let networkFee: String?
        let total: String?
        let pay: String?
        let rate: String?
    }

    let payTitle: String
    /// The chosen quote's charge, once there is one.
    let breakdown: Breakdown?
}

struct FundingProvidersViewModel: Equatable {
    struct Row: Equatable {
        enum Badge: Equatable {
            case unavailable(String)
            case lowestPrice
        }

        enum Price: Equatable {
            case pending
            case quoted(provider: String?, cash: String?)
            case unavailable
        }

        let providerId: ProductId
        let brand: FundingProviderBrand
        let isSelected: Bool
        let badge: Badge?
        let price: Price
    }

    let rows: [Row]
    /// Some provider has not answered yet.
    let isRefreshing: Bool
    /// When the core asks every provider again.
    let refreshAt: Date?
}

/// A network's or token's row: its name, its icon once read, else its
/// first letter.
struct FundingAssetMarkViewModel: Equatable, Identifiable {
    let id: String
    let name: String
    let monogram: String
    let icon: UIImage?
}

struct FundingNetworkViewModel: Equatable {
    struct Row: Equatable, Identifiable {
        let mark: FundingAssetMarkViewModel
        /// The minimum line of a network closed to the typed amount.
        let minimum: String?

        var id: String { mark.id }
    }

    let rows: [Row]
}

struct FundingTokenViewModel: Equatable {
    /// The network the user picked.
    let network: FundingAssetMarkViewModel?
    let tokens: [FundingAssetMarkViewModel]
}

struct FundingCountryViewModel: Equatable {
    /// The country the core detected, which the list leads with.
    let detected: PaymentCountry?
    let selected: PaymentCountry?
    /// Codes of the countries no provider takes payment from.
    let unsupported: Set<String>
    /// The minimum line of each country a provider showed one for, by code.
    let minimums: [String: String]
}

/// A titled value the user can copy.
struct FundingCopyLineViewModel: Equatable {
    let title: String
    let text: String
}

struct FundingDepositViewModel: Equatable {
    struct Crypto: Equatable {
        /// What the QR code carries: the payment link, else the address.
        let payload: String
        let amountTitle: String
        let amount: String?
        let addressTitle: String
        let address: String
    }

    enum Content: Equatable {
        case crypto(Crypto)
        case bank([FundingCopyLineViewModel])
        case error(String)
        case pending(String)
    }

    enum Action: Equatable {
        case close
        case cancel
        case none
    }

    let title: String
    let content: Content
    let action: Action
    /// The funds were seen or the session ended: progress takes over.
    let paymentSeen: Bool
}

struct FundingProgressViewModel: Equatable {
    struct CancelOffer: Equatable {
        let title: String
        let fee: String
        let refund: String
        /// The terms as shown, which accepting answers with.
        let terms: CancelTerms
    }

    struct Ended: Equatable {
        let session: FundingEndedSession
        let viewModel: FundingEndedViewModel
    }

    struct Note: Equatable {
        let text: String
        let isWarning: Bool
    }

    struct LiveLine: Equatable {
        let heardAt: Date?
        let detail: FundingDetail?
    }

    /// The rows under the steps, for the session's direction and rail.
    enum Details: Equatable {
        /// What was sent or will be received, opening the fees.
        case amount(title: String, amount: String?)
        case bank(Bank)
        case crypto(CryptoDeposit)
    }

    struct Bank: Equatable {
        let amountTitle: String
        let amount: String?
        let copyLines: [FundingCopyLineViewModel]
        let isPreparing: Bool
    }

    enum CryptoDeposit: Equatable {
        /// The funds were seen.
        case hidden
        case preparing
        /// The way back to the address to pay.
        case deposit(title: String, amount: String?)
    }

    struct Arrival: Equatable {
        let eta: String
        let rateNote: String?
    }

    let figure: String?
    let symbol: String
    /// Retrying or delayed: the session's icon and current step turn amber.
    let isSlow: Bool
    let steps: [FundingStepBar.Step]
    let stepsBetween: Int
    /// The line under the steps: where a cancel stands, else the band's.
    let note: Note?
    let liveLine: LiveLine?
    let details: Details
    /// The fees open from the amount row once a quote is chosen.
    let hasFees: Bool
    let arrival: Arrival?
    /// The cancel button's title, while the session can be cancelled.
    let cancelTitle: String?
    let cancelOffer: CancelOffer?
    let isAnsweringOffer: Bool
    /// When the session turns to taking longer, once started.
    let hasStarted: Bool
    let longerAtMs: UInt64?
    let ended: Ended?
}

struct FundingCancelConfirmViewModel: Equatable {
    let title: String
    let message: String
    let isCancelling: Bool
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
