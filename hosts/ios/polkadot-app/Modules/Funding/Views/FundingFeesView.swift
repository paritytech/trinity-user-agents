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
    func feeText(_ units: U128) -> String {
        if model.direction == .in {
            return model.quoteUnit.format(units)
        }
        return model.cash.label(model.cash.decimal(units))
    }

    func breakdown(_ quote: FundingQuote) -> some View {
        let total = (Decimal(string: quote.providerFee) ?? 0) + (Decimal(string: quote.networkFee) ?? 0)

        return VStack(spacing: DSSpacings.small) {
            line(String(localized: .Funding.feesProvider), feeText(quote.providerFee))
            line(String(localized: .Funding.feesNetwork), feeText(quote.networkFee))
            line(String(localized: .Funding.feesSwapping), String(localized: .Funding.feesVariable))
            Divider().overlay(Color.strokePrimary)
            line(String(localized: .Funding.feesTotal), feeText(NSDecimalNumber(decimal: total).stringValue))
            Divider().overlay(Color.strokePrimary)

            HStack(alignment: .firstTextBaseline) {
                Text(payTitle)
                    .typography(.bodyLarge)
                    .foregroundStyle(.fgSecondary)
                Spacer()
                Text(verbatim: feeText(quote.sendAmount))
                    .typography(.headlineSmall)
                    .foregroundStyle(.fgPrimary)
            }
            .padding(.top, DSSpacings.small)

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

    /// What one CASH is worth in the provider-side asset, from the quote.
    func rate(_ quote: FundingQuote) -> String? {
        let providerSide = model.quoteUnit.decimal(model.direction == .in ? quote.sendAmount : quote.receiveAmount)
        let cashSide = model.cash.decimal(model.direction == .in ? quote.receiveAmount : quote.sendAmount)
        guard cashSide > 0 else { return nil }

        let perCash = model.quoteUnit.format(decimal: (providerSide / cashSide).fundingRounded(scale: 4))
        return "\(model.cash.label(1)) ≈ \(perCash)"
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
