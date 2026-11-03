import SwiftUI
import DesignSystem
import ExternalAccessibility

/// The CASH card: a leather wallet with the balance stamped at its foot.
public struct AssetDetailsBalanceCard: View {
    let viewModel: ViewModel
    let isUpdating: Bool
    let isExpanded: Bool
    /// How visible the balance is, so it can fade as the card shrinks into
    /// the header.
    let balanceOpacity: Double

    public init(viewModel: ViewModel, isUpdating: Bool, isExpanded: Bool = false, balanceOpacity: Double = 1) {
        self.viewModel = viewModel
        self.isUpdating = isUpdating
        self.isExpanded = isExpanded
        self.balanceOpacity = balanceOpacity
    }

    public var body: some View {
        CashCardFace()
            .overlay(alignment: .topLeading) { updatingLabel }
            .overlay(alignment: .bottomLeading) { balance.opacity(balanceOpacity) }
            .accessibilityId(AccessibilityID.Wallet.cashCard)
    }

    @ViewBuilder
    private var updatingLabel: some View {
        if isUpdating {
            SwiftUI.Label {
                Text(.walletCardUpdatingBalance)
                    .textStyle(.body14Regular())
            } icon: {
                SpinningUpdateIcon()
            }
            .foregroundStyle(.white)
            .shadow(color: .black.opacity(0.25), radius: 1, y: 1)
            .padding(.top, Constants.balanceInset.bottom)
            .padding(.leading, Constants.balanceInset.leading)
        }
    }

    @ViewBuilder
    private var balance: some View {
        if let amount = viewModel.balance {
            HStack(alignment: .firstTextBaseline, spacing: Constants.unitSpacing) {
                Text(verbatim: amount)
                    .font(.system(size: 30, weight: .semibold))
                    .monospacedDigit()
                    .contentTransition(.numericText())
                if let symbol = viewModel.symbol {
                    Text(verbatim: symbol.lowercased())
                        .font(.system(size: 24, weight: .light).smallCaps())
                }
            }
            .foregroundStyle(.white)
            .shadow(color: .black.opacity(0.25), radius: 1, y: 1)
            .shimmering(active: isUpdating)
            .padding(.leading, Constants.balanceInset.leading)
            .padding(.bottom, Constants.balanceInset.bottom)
            .accessibilityElement(children: .combine)
            .accessibilityId(isExpanded ? AccessibilityID.Wallet.totalBalance : AccessibilityID.Wallet.cashCardBalance)
        }
    }
}

/// The card's face on its own, at the wallet's proportions: the leather wallet, lit by the phone's
/// motion. The Purse header draws it as a thumbnail once the card has shrunk away.
public struct CashCardFace: View {
    public init() {}

    public var body: some View {
        Image(.walletLeather)
            .resizable()
            .scaledToFit()
            .walletLeather(CashCardFace.wallet)
            .aspectRatio(CashCardFace.aspectRatio, contentMode: .fit)
    }

    /// The wallet artwork and where its brass button sits.
    public static let wallet = WalletLeatherImage.cashWallet
    /// The wallet artwork's own proportions.
    public static let aspectRatio: CGFloat = wallet.aspectRatio
}

public extension AssetDetailsBalanceCard {
    struct ViewModel {
        let balance: String?
        let readyBalance: String?
        let logo: UIImage?
        let symbol: String?

        public init(balance: String?, readyBalance: String?, logo: UIImage? = nil, symbol: String? = nil) {
            self.balance = balance
            self.readyBalance = readyBalance
            self.logo = logo
            self.symbol = symbol
        }
    }
}

private extension AssetDetailsBalanceCard {
    enum Constants {
        /// Where the balance sits on the 366 by 239 artwork.
        static let balanceInset = (leading: CGFloat(23), bottom: CGFloat(22))
        static let unitSpacing: CGFloat = 6
    }
}

#Preview {
    ZStack {
        Color.gray
        AssetDetailsBalanceCard(
            viewModel: AssetDetailsBalanceCard.ViewModel(balance: "$123", readyBalance: nil, symbol: "CASH"),
            isUpdating: true
        )
        .padding()
    }
}

#Preview("Expanded") {
    ZStack {
        Color.gray
        AssetDetailsBalanceCard(
            viewModel: AssetDetailsBalanceCard.ViewModel(balance: "$424.13", readyBalance: "100", symbol: "CASH"),
            isUpdating: false,
            isExpanded: true
        )
        .padding()
    }
}
