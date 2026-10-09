import DesignSystem
import PolkadotUI
import SwiftUI
import TrUAPIHost

/// What the chosen provider will charge and pay, before the session is handed
/// to it.
struct FundingSummaryView: View {
    @Bindable var model: FundingFlowModel

    var body: some View {
        VStack(spacing: DSSpacings.mediumIncreased) {
            FundingScreenHeader(title: title, onBack: model.back)

            VStack(spacing: DSSpacings.small) {
                headline
                feesLink
            }
            .padding(.top, DSSpacings.medium)

            VStack(spacing: DSSpacings.extraSmall) {
                if model.rail != .crypto {
                    countryRow
                }
                providerRow
                payoutRow
                arrivesRow
            }

            if let problem {
                Text(problem)
                    .typography(.bodySmall)
                    .foregroundStyle(.fgError)
                    .multilineTextAlignment(.center)
            }

            Spacer(minLength: 0)

            FundingPrimaryButton(
                title: String(localized: .Funding.continueAction),
                isEnabled: model.selectedQuote != nil,
                isLoading: model.isStarting,
                action: model.start
            )
        }
        .task { await expiryTimer() }
    }
}

private extension FundingSummaryView {
    var title: String {
        switch (model.direction, model.rail) {
        case (.in, .card): String(localized: .Funding.summaryCardTitle)
        case (.in, .bank): String(localized: .Funding.summaryBankTitle)
        case (.in, .crypto): String(localized: .Funding.summaryCryptoTitle)
        case (.out, .card): String(localized: .Funding.summaryWithdrawCardTitle)
        case (.out, .bank): String(localized: .Funding.summaryWithdrawBankTitle)
        case (.out, .crypto): String(localized: .Funding.summaryWithdrawCryptoTitle)
        }
    }

    /// What the user pays for value in, what they receive for value out.
    @ViewBuilder
    var headline: some View {
        if let quote = model.selectedQuote {
            let units = model.direction == .in ? quote.sendAmount : quote.receiveAmount
            Text(verbatim: model.quoteUnit.format(units))
                .typography(.displayLarge)
                .foregroundStyle(.fgPrimary)
                .lineLimit(1)
                .minimumScaleFactor(0.5)
        } else if model.isQuoting {
            FundingSkeleton(width: 160, height: 48)
        } else {
            Text(verbatim: "–")
                .typography(.displayLarge)
                .foregroundStyle(.fgTertiary)
        }
    }

    var feesLink: some View {
        Button {
            model.path.append(.fees)
        } label: {
            HStack(spacing: DSSpacings.extraSmall) {
                Text(feesCaption)
                    .typography(.bodySmall)
                Image(systemName: "chevron.right")
                    .font(.system(size: 11, weight: .semibold))
            }
            .foregroundStyle(.fgSecondary)
        }
        .buttonStyle(.plain)
        .disabled(model.selectedQuote == nil)
    }

    var feesCaption: String {
        switch (model.direction, model.rail) {
        case (.in, .card): String(localized: .Funding.summaryCardCaption)
        case (.in, _): String(localized: .Funding.summaryBankCaption)
        case (.out, _): String(localized: .Funding.summaryWithdrawCaption)
        }
    }

    var countryRow: some View {
        FundingValueRow(
            title: String(localized: .Funding.summaryCountry),
            action: { model.path.append(.country) },
            value: {
                if let country = model.country {
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
            action: { model.path.append(.providers) },
            value: {
                if let providerId = model.selectedProviderId {
                    let brand = model.brand(for: providerId)
                    HStack(spacing: DSSpacings.extraSmall) {
                        FundingProviderLogo(brand: brand, size: 20)
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
        let title =
            if model.direction == .in {
                String(localized: .Funding.summaryMinPayout)
            } else {
                String(localized: .Funding.summaryYouSend)
            }

        return FundingValueRow(title: title) {
            if let quote = model.selectedQuote {
                let units = model.direction == .in ? quote.receiveAmount : quote.sendAmount
                Text(verbatim: model.cash.label(model.cash.decimal(units)))
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
                if let eta = model.selectedQuote?.etaSecs {
                    Text(verbatim: FundingEta.text(seconds: eta))
                        .typography(.bodyLargeEmphasized)
                        .foregroundStyle(.fgPrimary)
                } else if model.isQuoting {
                    FundingSkeleton()
                } else {
                    Text(verbatim: "–")
                        .foregroundStyle(.fgSecondary)
                }
            }
            if model.rail == .bank, model.direction == .in {
                Text(.Funding.summaryBankRateNote(symbol: model.cash.symbol))
                    .typography(.captionSmall)
                    .foregroundStyle(.fgSecondary)
            }
        }
    }

    var problem: String? {
        if let failure = model.failure {
            return failure.failedText
        }
        if let startError = model.startError {
            return startError
        }
        guard model.noProviderQuoted else { return nil }

        switch model.unavailableIssue {
        case let .belowMinimum(minimum):
            return String(localized: .Funding.errorMinimum(amount: model.cash.label(minimum)))
        case let .aboveMaximum(maximum):
            return String(localized: .Funding.errorMaximum(amount: model.cash.label(maximum)))
        case .notEnoughBalance,
             nil:
            return model.quoteFailureText ?? String(localized: .Funding.errorNoProvider)
        }
    }

    func expiryTimer() async {
        while !Task.isCancelled {
            model.refreshIfExpired()
            try? await Task.sleep(for: .seconds(1))
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
