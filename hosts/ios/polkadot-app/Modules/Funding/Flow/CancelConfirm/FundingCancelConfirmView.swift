import DesignSystem
import PolkadotUI
import SwiftUI

/// Asks before cancelling: a deposit the user may already have paid, or a
/// session the provider is working on.
struct FundingCancelConfirmView: View {
    let viewModel: FundingCancelConfirmViewModel
    let presenter: FundingCancelConfirmPresenterProtocol

    var body: some View {
        VStack(spacing: DSSpacings.medium) {
            FundingScreenHeader(title: "", onBack: presenter.back)

            Text(viewModel.title)
                .typography(.headlineMedium)
                .foregroundStyle(.fgPrimary)
            Text(viewModel.message)
                .typography(.bodyMedium)
                .foregroundStyle(.fgPrimary)
                .multilineTextAlignment(.center)

            Spacer(minLength: 0)

            DSButton(
                .Funding.depositCancel,
                style: .destructive,
                expands: true,
                isLoading: viewModel.isCancelling,
                action: presenter.requestCancel
            )
            DSButton(.Funding.cancelKeep, expands: true, action: presenter.back)
        }
        .fundingScreen()
    }
}
