import DesignSystem
import PolkadotUI
import SwiftUI
import TrUAPIHost

/// How the chosen quote's charge breaks down.
struct FundingFeesView: View {
    let model: FundingFlowModel

    var body: some View {
        VStack(spacing: DSSpacings.mediumIncreased) {
            FundingScreenHeader(title: String(localized: .Funding.feesTitle), onBack: model.back)

            if let quote = model.chosenQuote {
                breakdown(quote)
            }

            Spacer(minLength: 0)

            FundingPrimaryButton(title: String(localized: .Funding.backAction), style: .secondary, action: model.back)
        }
    }
}

extension FundingFeesView {
    static func totalFee(_ quote: FundingQuote) -> Decimal {
        (Decimal(string: quote.providerFee) ?? 0) + (Decimal(string: quote.networkFee) ?? 0)
    }

    /// What the user pays before fees, in the unit of `sendAmount`: the
    /// amount and the fees add up to the total shown under them.
    static func net(_ quote: FundingQuote) -> Decimal {
        max(0, (Decimal(string: quote.sendAmount) ?? 0) - totalFee(quote))
    }

    static func units(_ value: Decimal) -> U128 {
        NSDecimalNumber(decimal: value).stringValue
    }
}

private extension FundingFeesView {
    /// Fees are counted in what the user pays: the ask's asset for value in,
    /// the balance for value out.
    func feeText(_ units: U128, of quote: FundingQuote) -> String? {
        if model.direction == .in {
            return model.quoteUnit(quote).format(units)
        }
        return model.cash.label(units: units)
    }

    func breakdown(_ quote: FundingQuote) -> some View {
        let total = Self.totalFee(quote)

        return VStack(spacing: DSSpacings.small) {
            feeText(Self.units(Self.net(quote)), of: quote).map { line(String(localized: .Funding.feesAmount), $0) }
            feeText(quote.providerFee, of: quote).map { line(String(localized: .Funding.feesProvider), $0) }
            feeText(quote.networkFee, of: quote).map { line(String(localized: .Funding.feesNetwork), $0) }
            line(String(localized: .Funding.feesSwapping), String(localized: .Funding.feesVariable))
            Divider().overlay(Color.strokePrimary)
            feeText(Self.units(total), of: quote).map { line(String(localized: .Funding.feesTotal), $0) }
            Divider().overlay(Color.strokePrimary)

            if let pay = feeText(quote.sendAmount, of: quote) {
                HStack(alignment: .firstTextBaseline) {
                    Text(payTitle)
                        .typography(.bodyLarge)
                        .foregroundStyle(.fgSecondary)
                    Spacer()
                    Text(verbatim: pay)
                        .typography(.headlineSmall)
                        .foregroundStyle(.fgPrimary)
                }
                .padding(.top, DSSpacings.small)
            }

            if let rate = rate(quote) {
                line(String(localized: .Funding.feesRate), rate, emphasized: false)
            }
        }
    }

    var payTitle: String {
        switch (model.direction, model.rail) {
        case (.in, .card): String(localized: .Funding.feesYouPay)
        case (.in, _): String(localized: .Funding.feesAmountToSend)
        case (.out, _): String(localized: .Funding.feesYouSend)
        }
    }

    /// What one CASH is worth in the provider-side asset, net of fees as the
    /// amount line above it.
    func rate(_ quote: FundingQuote) -> String? {
        let net = Self.units(Self.net(quote))
        let providerUnits = model.direction == .in ? net : quote.receiveAmount
        guard let providerSide = model.quoteUnit(quote).decimal(providerUnits),
              let cashSide = model.cash.decimal(model.direction == .in ? quote.receiveAmount : net),
              cashSide > 0
        else { return nil }

        let perCash = model.quoteUnit(quote).format(decimal: (providerSide / cashSide).fundingRounded(scale: 4))
        return String(localized: .Funding.feesRateValue(cash: model.cash.label(1), value: perCash))
    }

    func line(_ title: String, _ value: String, emphasized: Bool = true) -> some View {
        HStack {
            Text(title)
                .typography(.bodyLarge)
                .foregroundStyle(.fgSecondary)
            Spacer()
            Text(verbatim: value)
                .typography(emphasized ? .bodyLargeEmphasized : .bodyLarge)
                .foregroundStyle(emphasized ? Color.fgPrimary : Color.fgSecondary)
        }
    }
}
