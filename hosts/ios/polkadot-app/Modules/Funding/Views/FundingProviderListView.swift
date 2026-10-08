import DesignSystem
import PolkadotUI
import SwiftUI
import TrUAPIHost

/// Every provider's live quote for the ask, the cheapest marked, with how long
/// the prices hold before they are asked for again.
struct FundingProviderListView: View {
    let model: FundingFlowModel

    var body: some View {
        VStack(spacing: DSSpacings.medium) {
            FundingScreenHeader(title: String(localized: .Funding.providersTitle), onBack: model.back)

            TimelineView(.periodic(from: .now, by: 1)) { context in
                countdown(now: context.date)
                    .onChange(of: context.date) { _, now in model.refreshIfExpired(now: now) }
            }

            VStack(spacing: DSSpacings.extraSmall) {
                ForEach(model.servingCandidates, id: \.providerId) { candidate in
                    row(candidate.providerId)
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
        } else if let expiry = model.quoteExpiry {
            let remaining = max(0, Int(expiry.timeIntervalSince(now).rounded(.down)))
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

    func row(_ providerId: String) -> some View {
        let state = model.rows[providerId] ?? .pending
        let brand = model.brand(for: providerId)
        let isSelected = providerId == model.selectedProviderId

        return Button {
            model.chooseProvider(providerId)
        } label: {
            HStack(spacing: DSSpacings.smallIncreased) {
                FundingProviderLogo(brand: brand, size: 36)
                VStack(alignment: .leading, spacing: 2) {
                    Text(verbatim: brand.name)
                        .typography(.bodyMediumEmphasized)
                        .foregroundStyle(.fgPrimary)
                    badge(providerId: providerId, state: state)
                }
                Spacer()
                price(state)
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
    func badge(providerId: String, state: FundingQuoteState) -> some View {
        if case let .unavailable(reason) = state {
            Text(unavailableText(reason))
                .typography(.captionMedium)
                .foregroundStyle(.fgSecondary)
        } else if providerId == model.bestProviderId, model.quotedProviders.count > 1 {
            Text(.Funding.providersLowestPrice)
                .typography(.captionMedium)
                .foregroundStyle(.fgSuccess)
        }
    }

    @ViewBuilder
    func price(_ state: FundingQuoteState) -> some View {
        switch state {
        case .pending:
            VStack(alignment: .trailing, spacing: 4) {
                FundingSkeleton(width: 64)
                FundingSkeleton(width: 48, height: 10)
            }
        case let .quoted(quote):
            let providerSide = model.direction == .in ? quote.sendAmount : quote.receiveAmount
            let cashSide = model.direction == .in ? quote.receiveAmount : quote.sendAmount
            VStack(alignment: .trailing, spacing: 2) {
                Text(verbatim: "≈ " + model.quoteUnit.formatWithCode(providerSide))
                    .typography(.bodyMediumEmphasized)
                    .foregroundStyle(.fgPrimary)
                Text(.Funding.providersFor(amount: model.cash.label(model.cash.decimal(cashSide))))
                    .typography(.captionSmall)
                    .foregroundStyle(.fgSecondary)
            }
        case .unavailable:
            EmptyView()
        }
    }

    func unavailableText(_ reason: FundingQuoteUnavailable) -> String {
        switch reason {
        case .timeout:
            String(localized: .Funding.providersUnavailable)
        case let .refused(refusal):
            switch refusal {
            case let .belowMinimum(min):
                String(localized: .Funding.errorMinimum(amount: model.cash.label(model.cash.decimal(min))))
            case let .aboveMaximum(max):
                String(localized: .Funding.errorMaximum(amount: model.cash.label(model.cash.decimal(max))))
            case .countryUnsupported:
                String(localized: .Funding.providersCountryUnsupported)
            case .unavailable,
                 .other:
                String(localized: .Funding.providersUnavailable)
            }
        }
    }
}
