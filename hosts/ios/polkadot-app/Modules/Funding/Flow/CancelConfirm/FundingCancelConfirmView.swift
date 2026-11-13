import DesignSystem
import PolkadotUI
import SwiftUI

/// Asks before cancelling: a deposit the user may already have paid, or a
/// session the provider is working on.
struct FundingCancelConfirmView: View {
    let model: FundingScreenModel
    let presenter: FundingCancelConfirmPresenterProtocol

    var body: some View {
        VStack(spacing: DSSpacings.medium) {
            FundingScreenHeader(title: "", onBack: presenter.back)

            Text(model.direction == .in ? .Funding.cancelTitle : .Funding.cancelTitleWithdraw)
                .typography(.headlineMedium)
                .foregroundStyle(.fgPrimary)
            Text(message)
                .typography(.bodyMedium)
                .foregroundStyle(.fgPrimary)
                .multilineTextAlignment(.center)

            Spacer(minLength: 0)

            DSButton(
                .Funding.depositCancel,
                style: .destructive,
                expands: true,
                isLoading: model.isCancelling,
                action: presenter.requestCancel
            )
            DSButton(.Funding.cancelKeep, expands: true, action: presenter.back)
        }
    }

    private var message: LocalizedStringResource {
        guard model.isUnpaidTopUp else { return .Funding.cancelBodyProvider }
        return model.rail == .bank ? .Funding.cancelBodyBank : .Funding.cancelBody
    }
}
