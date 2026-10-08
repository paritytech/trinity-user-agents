import DesignSystem
import SwiftUI

/// The overlay sheet: the amount screen, with every later screen pushed over
/// it from the model's path.
struct FundingSheetView: View {
    @Bindable var model: FundingFlowModel

    var body: some View {
        NavigationStack(path: $model.path) {
            screen { FundingAmountView(model: model) }
                .navigationDestination(for: FundingFlowModel.Screen.self) { destination in
                    screen { content(for: destination) }
                }
        }
        .fundingToast($model.toast)
    }
}

private extension FundingSheetView {
    func screen(@ViewBuilder _ content: () -> some View) -> some View {
        content()
            .padding(.horizontal, DSSpacings.mediumIncreased)
            .padding(.top, DSSpacings.mediumIncreased)
            .padding(.bottom, DSSpacings.small)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .background(Color.bgSurfaceContainer.ignoresSafeArea())
            .toolbar(.hidden, for: .navigationBar)
    }

    @ViewBuilder
    func content(for destination: FundingFlowModel.Screen) -> some View {
        switch destination {
        case .summary:
            FundingSummaryView(model: model)
        case .fees:
            FundingFeesView(model: model)
        case .country:
            FundingCountryView(model: model)
        case .providers:
            FundingProviderListView(model: model)
        case .network:
            FundingNetworkView(model: model)
        case .token:
            FundingTokenView(model: model)
        case .deposit:
            FundingDepositView(model: model)
        case .cancelConfirm:
            FundingCancelConfirmView(model: model)
        }
    }
}
