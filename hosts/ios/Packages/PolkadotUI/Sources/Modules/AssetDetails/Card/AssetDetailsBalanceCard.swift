import SwiftUI
import DesignSystem
import ExternalAccessibility

public struct AssetDetailsBalanceCard: View {
    let viewModel: ViewModel
    let isUpdating: Bool
    let isExpanded: Bool

    public init(viewModel: ViewModel, isUpdating: Bool, isExpanded: Bool = false) {
        self.viewModel = viewModel
        self.isUpdating = isUpdating
        self.isExpanded = isExpanded
    }

    /// The card is a leather wallet, so it has the wallet's own proportions and outline rather
    /// than a bordered rectangle's.
    public var body: some View {
        content
            .accessibilityId(AccessibilityID.Wallet.cashCard)
            .aspectRatio(Constants.wallet.aspectRatio, contentMode: .fit)
    }

    /// The total, which sits at the foot of the card in both of its states.
    private func total(_ balance: String) -> some View {
        DSAmount(amount: balance, symbol: viewModel.symbol, typography: .headlineMedium)
            .shimmering(active: isUpdating)
            .accessibilityId(AccessibilityID.Wallet.totalBalance)
    }

    private var content: some View {
        ZStack(alignment: .leading) {
            Image(.walletLeather)
                .resizable()
                .scaledToFit()
                .walletLeather(Constants.wallet)

            VStack(alignment: .leading, spacing: 0) {
                HStack(spacing: 5) {
                    Image(uiImage: viewModel.logo ?? .cashLogo)
                        .resizable()
                        .scaledToFit()
                        .frame(height: Constants.logoHeight)

                    Spacer()

                    if !isExpanded, let balance = viewModel.balance {
                        DSAmount(amount: balance, symbol: viewModel.symbol, typography: .titleLarge)
                            .foregroundStyle(Color.fgStaticWhite)
                            .transition(.opacity)
                            .accessibilityId(AccessibilityID.Wallet.cashCardBalance)
                    }
                }
                .animation(.easeInOut, value: isExpanded)
                Spacer()

                if isUpdating {
                    SwiftUI.Label {
                        Text(.walletCardUpdatingBalance)
                            .textStyle(.body14Regular())
                    } icon: {
                        SpinningUpdateIcon()
                    }
                    .foregroundStyle(.white)
                }

                if isExpanded, let readyBalance = viewModel.readyBalance, let balance = viewModel.balance {
                    VStack(alignment: .leading, spacing: DSSpacings.small) {
                        VStack(alignment: .leading, spacing: 0) {
                            Text(.walletCardReady)
                                .typography(.bodyMedium)
                                .foregroundStyle(Color.fgStaticWhite)
                            Text(readyBalance)
                                .typography(.bodyMedium)
                        }

                        VStack(alignment: .leading, spacing: 0) {
                            Text(.walletCardTotalBalance)
                                .typography(.bodyMedium)
                                .foregroundStyle(Color.fgStaticWhite)
                            total(balance)
                        }
                    }
                } else if let balance = viewModel.balance {
                    total(balance)
                }
            }
            .foregroundStyle(Color.white)
            .padding(.top, DSSpacings.mediumIncreased)
            .padding(.bottom, DSSpacings.mediumIncreased)
            .padding(.leading, DSSpacings.large)
            .padding(.trailing, 22)
        }
    }
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
        static let logoHeight: CGFloat = 32
        static let wallet = WalletLeatherImage.cashWallet
    }
}

#Preview {
    ZStack {
        Color.gray
        AssetDetailsBalanceCard(
            viewModel: AssetDetailsBalanceCard.ViewModel(balance: "$123", readyBalance: nil, symbol: "CASH"),
            isUpdating: true
        )
    }
}

#Preview("Expanded with Ready balance") {
    ZStack {
        Color.gray
        AssetDetailsBalanceCard(
            viewModel: AssetDetailsBalanceCard.ViewModel(balance: "$456", readyBalance: "100", symbol: "CASH"),
            isUpdating: false,
            isExpanded: true
        )
    }
}
