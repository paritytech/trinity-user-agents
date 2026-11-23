import DesignSystem
import PolkadotUI
import SwiftUI
import TrUAPIHost

/// Every provider's live quote for the ask, the one the core marks best
/// tagged, with how long until the core asks every provider again.
struct FundingProviderListView: View {
    let viewModel: FundingProvidersViewModel
    let presenter: FundingProvidersPresenterProtocol

    var body: some View {
        VStack(spacing: DSSpacings.medium) {
            FundingScreenHeader(title: String(localized: .Funding.providersTitle), onBack: presenter.back)

            TimelineView(.periodic(from: .now, by: 1)) { context in
                countdown(now: context.date)
            }

            VStack(spacing: DSSpacings.extraSmall) {
                ForEach(viewModel.rows, id: \.providerId) { row in
                    self.row(row)
                }
            }

            Spacer(minLength: 0)
        }
        .fundingScreen()
    }
}

private extension FundingProviderListView {
    @ViewBuilder
    func countdown(now: Date) -> some View {
        if viewModel.isRefreshing {
            HStack {
                FundingSkeleton(width: 120)
                Spacer()
                FundingSkeleton(width: 32)
            }
        } else if let refreshAt = viewModel.refreshAt {
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

    func row(_ row: FundingProvidersViewModel.Row) -> some View {
        let isQuoted = if case .quoted = row.price { true } else { false }
        let isPending = row.price == .pending

        return Button {
            presenter.chooseProvider(row.providerId)
        } label: {
            HStack(spacing: DSSpacings.smallIncreased) {
                FundingProviderLogo(brand: row.brand)
                VStack(alignment: .leading, spacing: DSSpacings.extraTiny) {
                    Text(verbatim: row.brand.name)
                        .typography(.bodyMediumEmphasized)
                        .foregroundStyle(.fgPrimary)
                    badge(row.badge)
                }
                Spacer()
                price(row.price)
            }
            .padding(DSSpacings.small)
            .background(
                row.isSelected ? Color.bgSurfaceNested : Color.clear,
                in: RoundedRectangle(cornerRadius: DSRadii.mediumIncreased)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!isQuoted)
        .opacity(!isQuoted && !isPending ? 0.5 : 1)
    }

    @ViewBuilder
    func badge(_ badge: FundingProvidersViewModel.Row.Badge?) -> some View {
        switch badge {
        case let .unavailable(text):
            Text(text)
                .typography(.captionMedium)
                .foregroundStyle(.fgSecondary)
        case .lowestPrice:
            Text(.Funding.providersLowestPrice)
                .typography(.captionMedium)
                .foregroundStyle(.fgSuccess)
        case nil:
            EmptyView()
        }
    }

    @ViewBuilder
    func price(_ price: FundingProvidersViewModel.Row.Price) -> some View {
        switch price {
        case .pending:
            VStack(alignment: .trailing, spacing: DSSpacings.tiny) {
                FundingSkeleton(width: 64)
                FundingSkeleton(width: 48, height: 10)
            }
        case let .quoted(providerAmount, cashAmount):
            VStack(alignment: .trailing, spacing: DSSpacings.extraTiny) {
                if let providerAmount {
                    Text(verbatim: providerAmount)
                        .typography(.bodyMediumEmphasized)
                        .foregroundStyle(.fgPrimary)
                }
                if let cashAmount {
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
