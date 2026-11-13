import SwiftUI
import DesignSystem
import ExternalAccessibility
import PolkadotUI

struct BackupViewLayout: View {
    let viewModel: BackupViewModel
    let presenter: BackupPresenterProtocol

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: DSSpacings.large) {
                if let restoredBalance = viewModel.restoredBalance {
                    BackupRestoredBalanceCard(
                        viewModel: restoredBalance,
                        onUpdate: { presenter.onRestoredBalanceUpdate() },
                        onClose: { presenter.onRestoredBalanceClose() },
                        onWhyUpdate: { presenter.onRestoredBalanceWhyUpdate() }
                    )
                }
                BackupRecoverySection(statusType: viewModel.statusType, presenter: presenter)
            }
            .padding(.horizontal, DSSpacings.mediumIncreased)
            .padding(.top, DSSpacings.extraMedium)
        }
        .scrollIndicators(.hidden)
    }
}

private struct BackupRecoverySection: View {
    let statusType: BackupViewModel.BackupStatusType?
    let presenter: BackupPresenterProtocol

    var body: some View {
        VStack(alignment: .leading, spacing: DSSpacings.extraMedium) {
            DSCaption(String(localized: .backupSectionRecovery))
            VStack(spacing: 0) {
                DSMenuListItem(
                    id: "secretPhrase",
                    title: String(localized: .backupCellSecretPhrase),
                    icon: .iconKey,
                    rightSlot: .chevron,
                    accessibilityId: AccessibilityID.Settings.viewRecoveryPhraseButton,
                    action: { presenter.selectSecretPhrase() }
                )
                .dsMenuListItemPosition(.first)
                DSMenuListItem(
                    id: "cloudBackup",
                    title: String(localized: .backupCellCloudBackup),
                    icon: .iconCloud,
                    rightSlot: cloudRightSlot,
                    accessibilityId: AccessibilityID.Backup.toIcloudButton,
                    action: { presenter.selectCloudBackup() }
                )
                .dsMenuListItemPosition(.last)
            }
        }
    }
}

private extension BackupRecoverySection {
    var cloudRightSlot: DSMenuListItemRightSlot.Style? {
        switch statusType {
        case .created:
            .labelChevron(String(localized: .backupCellCloudOn))
        case .notFound,
             .cloudIsOff:
            .labelChevron(String(localized: .backupCellCloudOff))
        case nil:
            nil
        }
    }
}
