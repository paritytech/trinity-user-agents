import DesignSystem
import PolkadotUI
import SwiftUI
import TrUAPIHost

/// How the chosen quote's charge breaks down.
struct FundingFeesView: View {
    let model: FundingScreenModel
    let presenter: FundingFeesPresenterProtocol

    var body: some View {
        VStack(spacing: DSSpacings.mediumIncreased) {
            FundingScreenHeader(title: String(localized: .Funding.feesTitle), onBack: presenter.back)

            if let quote = model.chosenQuote {
                breakdown(quote)
            }

            Spacer(minLength: 0)

            DSButton(.Funding.backAction, style: .secondary, expands: true, action: presenter.back)
        }
    }
}

private extension FundingFeesView {
    /// Fees are counted in what the user pays: the ask's asset for value in,
    /// the balance for value out.
    func feeText(_ units: U128, of quote: FundingQuote) -> String? {
        if model.direction == .in {
            return model.state.quoteUnit(quote).format(units)
        }
        return model.cash.label(units: units)
    }

    func breakdown(_ quote: FundingQuote) -> some View {
        let fees = FundingFees(quote: quote, direction: model.direction, cashPrecision: model.cash.precision)
        let total = fees.total.units

        return VStack(spacing: DSSpacings.small) {
            feeText(fees.net.units, of: quote).map { line(String(localized: .Funding.feesAmount), $0) }
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
    /// amount line above it.
    func rate(_ fees: FundingFees, of quote: FundingQuote) -> String? {
        guard let providerSide = model.state.quoteUnit(quote).decimal(fees.rateNum.units),
              let cashSide = model.cash.decimal(fees.rateDen.units),
              cashSide > 0
        else { return nil }

        let perCash = model.state.quoteUnit(quote).format(decimal: (providerSide / cashSide).fundingRounded(scale: 4))
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

/// What a quote's fees come to, in the unit of `sendAmount`: what the user
/// pays less the fees, which with them adds up to the total, and the fees
/// together. A withdrawal's are charged in CASH and shown rounded up to the
/// cent, so the cost is never under-stated. The rate sets the provider's
/// asset against CASH, net of fees.
struct FundingFees: Equatable {
    let net: Decimal
    let total: Decimal
    /// The provider's asset, in its smallest unit, per ``rateDen`` CASH units.
    let rateNum: Decimal
    let rateDen: Decimal

    init(quote: FundingQuote, direction: FundingDirection, cashPrecision: Int16) {
        let fees = Decimal(units: quote.providerFee) + Decimal(units: quote.networkFee)
        let receive = Decimal(units: quote.receiveAmount)
        net = max(0, Decimal(units: quote.sendAmount) - fees)
        switch direction {
        case .in:
            total = fees
            rateNum = net
            rateDen = receive
        case .out:
            // Fees out are rounded up to the cent, so the user is never shown less than is taken.
            let cent = pow(Decimal(10), Int(max(cashPrecision - 2, 0)))
            total = (fees / cent).fundingRounded(scale: 0, mode: .up) * cent
            rateNum = receive
            rateDen = net
        }
    }
}

private extension Decimal {
    init(units: U128) {
        self = Decimal(string: units) ?? 0
    }

    var units: U128 {
        NSDecimalNumber(decimal: self).stringValue
    }
}
