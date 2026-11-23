import DesignSystem
import PolkadotUI
import SwiftUI
import TrUAPIHost

/// Every provider's live quote for the ask, the one the core marks best
/// tagged, with how long until the core asks every provider again.
struct FundingProviderListView: View {
    let model: FundingScreenModel<FundingProvidersViewModel>
    let presenter: FundingProvidersPresenterProtocol

    var body: some View {
        VStack(spacing: DSSpacings.medium) {
            FundingScreenHeader(title: String(localized: .Funding.providersTitle), onBack: presenter.back)

            TimelineView(.periodic(from: .now, by: 1)) { context in
                countdown(now: context.date)
            }

            VStack(spacing: DSSpacings.extraSmall) {
                ForEach(model.listedProviderIds, id: \.self) { providerId in
                    row(providerId)
                }
            }

            Spacer(minLength: 0)
        }
    }
}

private extension FundingProviderListView {
    @ViewBuilder
    func countdown(now: Date) -> some View {
        if model.rows.values.contains(where: \.isPending) {
            HStack {
                FundingSkeleton(width: 120)
                Spacer()
                FundingSkeleton(width: 32)
            }
        } else if let refreshAt = model.quotesRefreshAt {
            let remaining = max(0, Int(refreshAt.timeIntervalSince(now).rounded(.up)))
            HStack {
                Text(.Funding.providersRateUpdatedIn)
                    .typography(.bodySmall)
                    .foregroundStyle(.fgSecondary)
                Spacer()
                Text(verbatim: String(format: "%d:%02d", remaining / 60, remaining % 60))
                    .typography(.labelMediumEmphasized)
                    .foregroundStyle(color(remaining: remaining))
                    .monospacedDigit()
            }
        }
    }

    func color(remaining: Int) -> Color {
        switch remaining {
        case ...10: .fgError
        case ...20: .fgWarning
        default: .fgPrimary
        }
    }

    func row(_ providerId: ProductId) -> some View {
        let state = model.rows[providerId] ?? .pending
        let brand = model.state.brand(for: providerId)
        let isSelected = providerId == model.selectedProviderId

        return Button {
            presenter.chooseProvider(providerId)
        } label: {
            HStack(spacing: DSSpacings.smallIncreased) {
                FundingProviderLogo(brand: brand)
                VStack(alignment: .leading, spacing: 2) {
                    Text(verbatim: brand.name)
                        .typography(.bodyMediumEmphasized)
                        .foregroundStyle(.fgPrimary)
                    badge(providerId: providerId, state: state)
                }
                Spacer()
                price(providerId: providerId, state: state)
            }
            .padding(DSSpacings.small)
            .background(isSelected ? Color.bgSurfaceNested : Color.clear, in: RoundedRectangle(cornerRadius: 16))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(state.quote == nil)
        .opacity(state.quote == nil && !state.isPending ? 0.5 : 1)
    }

    @ViewBuilder
    func badge(providerId: ProductId, state: FundingQuoteState) -> some View {
        if case .unavailable = state, let unavailable = model.viewModel.unavailable[providerId] {
            Text(unavailable)
                .typography(.captionMedium)
                .foregroundStyle(.fgSecondary)
        } else if providerId == model.bestProviderId, model.quotedProviders.count > 1 {
            Text(.Funding.providersLowestPrice)
                .typography(.captionMedium)
                .foregroundStyle(.fgSuccess)
        }
    }

    @ViewBuilder
    func price(providerId: ProductId, state: FundingQuoteState) -> some View {
        switch state {
        case .pending:
            VStack(alignment: .trailing, spacing: 4) {
                FundingSkeleton(width: 64)
                FundingSkeleton(width: 48, height: 10)
            }
        case .quoted:
            let price = model.viewModel.prices[providerId]
            VStack(alignment: .trailing, spacing: 2) {
                if let providerAmount = price?.provider {
                    Text(verbatim: providerAmount)
                        .typography(.bodyMediumEmphasized)
                        .foregroundStyle(.fgPrimary)
                }
                if let cashAmount = price?.cash {
                    Text(verbatim: cashAmount)
                        .typography(.captionSmall)
                        .foregroundStyle(.fgSecondary)
                }
            }
        case .unavailable:
            EmptyView()
        }
    }
}
