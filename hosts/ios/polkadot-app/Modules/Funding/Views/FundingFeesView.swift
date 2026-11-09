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

private extension FundingFeesView {
    /// Fees are counted in what the user pays: the ask's asset for value in,
    /// the balance for value out.
    func feeText(_ units: U128, of quote: FundingQuote) -> String? {
        if model.direction == .in {
            return model.quoteUnit(quote).format(units)
        }
        return model.cash.label(units: units)
    }

    /// The amounts are the core's: what the user pays before fees, which with
    /// the fees adds up to the total under them, and a withdrawal's total
    /// rounded up to the cent so it never under-states the cost.
    func breakdown(_ quote: FundingQuote) -> some View {
        let fees = fundingFeeBreakdown(quote: quote, direction: model.direction)
        let total = model.direction == .out ? fees.totalFeeRoundedUp : fees.totalFee

        return VStack(spacing: DSSpacings.small) {
            feeText(fees.net, of: quote).map { line(String(localized: .Funding.feesAmount), $0) }
            feeText(quote.providerFee, of: quote).map { line(String(localized: .Funding.feesProvider), $0) }
            feeText(quote.networkFee, of: quote).map { line(String(localized: .Funding.feesNetwork), $0) }
            line(String(localized: .Funding.feesSwapping), String(localized: .Funding.feesVariable))
            Divider().overlay(Color.strokePrimary)
            feeText(total, of: quote).map { line(String(localized: .Funding.feesTotal), $0) }
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

            if let rate = rate(fees, of: quote) {
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
    /// amount line above it: the core's ratio of provider-side to CASH units.
    func rate(_ fees: FundingFeeBreakdown, of quote: FundingQuote) -> String? {
        guard let providerSide = model.quoteUnit(quote).decimal(fees.rateNum),
              let cashSide = model.cash.decimal(fees.rateDen),
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
