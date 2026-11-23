import DesignSystem
import PolkadotUI
import SwiftUI
import TrUAPIHost

/// What the chosen provider will charge and pay, before the session is handed
/// to it.
struct FundingSummaryView: View {
    let viewModel: FundingSummaryViewModel
    let presenter: FundingSummaryPresenterProtocol

    var body: some View {
        VStack(spacing: DSSpacings.mediumIncreased) {
            FundingScreenHeader(title: viewModel.title, onBack: presenter.back)

            VStack(spacing: DSSpacings.small) {
                headline
                feesLink
            }
            .padding(.top, DSSpacings.medium)

            VStack(spacing: DSSpacings.extraSmall) {
                if viewModel.showsCountry {
                    countryRow
                }
                providerRow
                payoutRow
                arrivesRow
            }

            if let problem = viewModel.problem {
                Text(problem)
                    .typography(.bodySmall)
                    .foregroundStyle(.fgError)
                    .multilineTextAlignment(.center)
            }

            Spacer(minLength: 0)

            DSButton(.Funding.continueAction, expands: true, isLoading: viewModel.isStarting, action: presenter.start)
                .disabled(!viewModel.hasQuote)
        }
        .fundingScreen()
    }
}

private extension FundingSummaryView {
    /// What the user pays for value in, what they receive for value out.
    @ViewBuilder
    var headline: some View {
        if let figure = viewModel.headline {
            Text(verbatim: figure)
                .typography(.displayLarge)
                .foregroundStyle(.fgPrimary)
                .lineLimit(1)
                .minimumScaleFactor(0.5)
        } else if viewModel.isQuoting {
            FundingSkeleton(width: 160, height: 48)
        } else {
            Text(verbatim: "–")
                .typography(.displayLarge)
                .foregroundStyle(.fgTertiary)
        }
    }

    var feesLink: some View {
        Button {
            presenter.showFees()
        } label: {
            HStack(spacing: DSSpacings.extraSmall) {
                Text(viewModel.feesCaption)
                    .typography(.bodySmall)
                Image(systemName: "chevron.right")
                    .font(.system(size: 11, weight: .semibold))
            }
            .foregroundStyle(.fgSecondary)
        }
        .buttonStyle(.plain)
        .disabled(!viewModel.hasQuote)
    }

    var countryRow: some View {
        FundingValueRow(
            title: String(localized: .Funding.summaryCountry),
            action: presenter.showCountry,
            value: {
                if let country = viewModel.country {
                    HStack(spacing: DSSpacings.extraSmall) {
                        Text(verbatim: country.flag)
                        Text(verbatim: country.name)
                            .typography(.bodyLargeEmphasized)
                            .foregroundStyle(.fgPrimary)
                    }
                } else {
                    Text(.Funding.summaryCountryChoose)
                        .typography(.bodyLargeEmphasized)
                        .foregroundStyle(.fgPrimary)
                }
            }
        )
    }

    var providerRow: some View {
        FundingValueRow(
            title: String(localized: .Funding.summaryProvider),
            action: presenter.showProviders,
            value: {
                if let brand = viewModel.provider {
                    HStack(spacing: DSSpacings.extraSmall) {
                        FundingProviderLogo(brand: brand, size: .s20)
                        Text(verbatim: brand.name)
                            .typography(.bodyLargeEmphasized)
                            .foregroundStyle(.fgPrimary)
                    }
                } else {
                    FundingSkeleton()
                }
            }
        )
    }

    var payoutRow: some View {
        FundingValueRow(title: viewModel.payoutTitle) {
            if let payout = viewModel.payout {
                Text(verbatim: payout)
                    .typography(.bodyLargeEmphasized)
                    .foregroundStyle(.fgPrimary)
            } else {
                FundingSkeleton()
            }
        }
    }

    var arrivesRow: some View {
        VStack(alignment: .trailing, spacing: 2) {
            FundingValueRow(title: String(localized: .Funding.summaryArrives)) {
                if let eta = viewModel.eta {
                    Text(verbatim: eta)
                        .typography(.bodyLargeEmphasized)
                        .foregroundStyle(.fgPrimary)
                } else if viewModel.isQuoting {
                    FundingSkeleton()
                } else {
                    Text(verbatim: "–")
                        .foregroundStyle(.fgSecondary)
                }
            }
            if let bankRateNote = viewModel.bankRateNote {
                Text(verbatim: bankRateNote)
                    .typography(.captionSmall)
                    .foregroundStyle(.fgSecondary)
            }
        }
    }
}

/// When the provider says the funds arrive.
enum FundingEta {
    static func text(seconds: UInt64) -> String {
        let minutes = max(1, Int((Double(seconds) / 60).rounded(.up)))
        if minutes < 60 {
            return String(localized: .Funding.etaMinutes(count: minutes))
        }
        let hours = Int((Double(minutes) / 60).rounded(.up))
        if hours < 24 {
            return String(localized: .Funding.etaHours(count: hours))
        }
        return String(localized: .Funding.etaDays(count: Int((Double(hours) / 24).rounded(.up))))
    }
}
