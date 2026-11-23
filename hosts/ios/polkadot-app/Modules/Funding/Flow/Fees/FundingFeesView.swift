import DesignSystem
import PolkadotUI
import SubstrateSdk
import SwiftUI
import TrUAPIHost

/// How the chosen quote's charge breaks down.
struct FundingFeesView: View {
    let model: FundingScreenModel<FundingFeesViewModel?>
    let presenter: FundingFeesPresenterProtocol

    var body: some View {
        VStack(spacing: DSSpacings.mediumIncreased) {
            FundingScreenHeader(title: String(localized: .Funding.feesTitle), onBack: presenter.back)

            if let fees = model.viewModel {
                breakdown(fees)
            }

            Spacer(minLength: 0)

            DSButton(.Funding.backAction, style: .secondary, expands: true, action: presenter.back)
        }
    }
}

private extension FundingFeesView {
    func breakdown(_ fees: FundingFeesViewModel) -> some View {
        VStack(spacing: DSSpacings.small) {
            fees.amount.map { line(String(localized: .Funding.feesAmount), $0) }
            fees.providerFee.map { line(String(localized: .Funding.feesProvider), $0) }
            fees.networkFee.map { line(String(localized: .Funding.feesNetwork), $0) }
            line(String(localized: .Funding.feesSwapping), String(localized: .Funding.feesVariable))
            Divider().overlay(Color.strokePrimary)
            fees.total.map { line(String(localized: .Funding.feesTotal), $0) }
            Divider().overlay(Color.strokePrimary)

            if let pay = fees.pay {
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

            if let rate = fees.rate {
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
    let net: Balance
    let total: Balance
    /// The provider's asset, in its smallest unit, per ``rateDen`` CASH units.
    let rateNum: Balance
    let rateDen: Balance

    init(quote: FundingQuote, direction: FundingDirection, cashPrecision: Int16) {
        let fees = (Balance(units: quote.providerFee) ?? 0) + (Balance(units: quote.networkFee) ?? 0)
        let send = Balance(units: quote.sendAmount) ?? 0
        let receive = Balance(units: quote.receiveAmount) ?? 0
        net = send > fees ? send - fees : 0
        switch direction {
        case .in:
            total = fees
            rateNum = net
            rateDen = receive
        case .out:
            // Fees out are rounded up to the cent, so the user is never shown less than is taken.
            let cent = Balance(10).power(Int(max(cashPrecision - 2, 0)))
            total = (fees + cent - 1) / cent * cent
            rateNum = receive
            rateDen = net
        }
    }
}
