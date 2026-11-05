import SwiftUI
import ExternalAccessibility
import PolkadotUI
import DesignSystem

struct BackupRestoredBalanceViewModel: Equatable {
    struct Balance: Equatable {
        let total: String
        let availableNow: String?
        let symbol: String?
    }

    let balance: Balance?
    let isUpdating: Bool
}

struct BackupRestoredBalanceCard: View {
    let viewModel: BackupRestoredBalanceViewModel

    var onUpdate: (() -> Void)?
    var onClose: (() -> Void)?
    var onWhyUpdate: (() -> Void)?

    var body: some View {
        WalletCardContainer(
            color: Color.bgSurfaceContainer,
            contentPadding: DSSpacings.mediumIncreased
        ) {
            content
        }
    }

    private var content: some View {
        VStack(alignment: .leading, spacing: DSSpacings.mediumIncreased) {
            if let balance = viewModel.balance {
                balanceBlock(balance)
            }

            textContent

            HStack {
                DSButton(
                    .Common.close,
                    style: .secondary,
                    expands: true
                ) {
                    onClose?()
                }
                .accessibilityId(AccessibilityID.Wallet.backupNotificationCloseButton)
                DSButton(
                    .Common.update,
                    style: .primary,
                    expands: true
                ) {
                    onUpdate?()
                }
            }
            .disabled(viewModel.isUpdating)
        }
    }

    private func balanceBlock(_ balance: BackupRestoredBalanceViewModel.Balance) -> some View {
        VStack(alignment: .leading, spacing: DSSpacings.tiny) {
            if viewModel.isUpdating {
                Label {
                    Text(.BalanceSync.balanceUpdating)
                        .typography(.bodySmall)
                } icon: {
                    SpinningUpdateIcon()
                }
                .foregroundStyle(.fgSecondary)
            } else if let availableNow = balance.availableNow {
                HStack(spacing: DSSpacings.tiny) {
                    Text(availableNow)
                        .typography(.bodySmall)
                        .foregroundStyle(.fgPrimary)
                    Text(.BalanceSync.balanceAvailable)
                        .typography(.bodySmall)
                        .foregroundStyle(.fgSecondary)
                }
            }

            DSAmount(amount: balance.total, symbol: balance.symbol, typography: .headlineMedium)
                .foregroundStyle(.fgPrimary)
                .opacity(viewModel.isUpdating ? Constants.updatingOpacity : 1)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var textContent: some View {
        VStack(alignment: .leading, spacing: DSSpacings.small) {
            Label {
                Text(.BalanceSync.notificationTitle)
                    .typography(.titleLarge)
                    .multilineTextAlignment(.leading)
                    .fixedSize(horizontal: false, vertical: true)
            } icon: {
                Image(.iconInfo20)
                    .renderingMode(.template)
                    .resizable()
                    .frame(width: Constants.iconSize, height: Constants.iconSize)
            }
            .foregroundStyle(.fgWarning)

            Text(.BalanceSync.notificationDescription)
                .typography(.bodyMedium)
                .foregroundStyle(.fgPrimary)
                .multilineTextAlignment(.leading)
                .fixedSize(horizontal: false, vertical: true)

            Button { onWhyUpdate?() } label: {
                HStack(spacing: 0) {
                    Text(.BalanceSync.notificationWhy)
                        .typography(.bodyMedium)
                    Image(.iconArrowRight20)
                        .renderingMode(.template)
                }
            }
            .foregroundStyle(.fgTertiary)
            .frame(height: Constants.linkHeight)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private extension BackupRestoredBalanceCard {
    enum Constants {
        static let iconSize: CGFloat = 20
        static let linkHeight: CGFloat = 28
        static let updatingOpacity: CGFloat = 0.4
    }
}
