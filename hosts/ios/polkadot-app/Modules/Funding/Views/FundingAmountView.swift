import DesignSystem
import PolkadotUI
import SwiftUI
import TrUAPIHost

/// The overlay's first screen: which rail, and how much.
struct FundingAmountView: View {
    @Bindable var model: FundingFlowModel

    private static let presets: [Decimal] = [10, 50, 100]

    var body: some View {
        VStack(spacing: DSSpacings.mediumIncreased) {
            FundingScreenHeader(title: title, leadingTitle: true, onBack: nil) {
                FundingCircleButton(systemImage: "clock.arrow.circlepath", action: model.close)
            }

            FundingRailTabs(selected: model.rail, available: model.availableRails) { rail in
                model.selectRail(rail)
            }

            VStack(spacing: DSSpacings.extraSmall) {
                if model.direction == .out, let spendable = model.spendable {
                    Text(.Funding.amountAvailable(amount: model.cash.label(spendable)))
                        .typography(.captionMedium)
                        .foregroundStyle(.fgSecondary)
                }
                amountFigure
                captionLine
            }
            .frame(maxWidth: .infinity)

            if model.direction == .in {
                presets
            }

            Spacer(minLength: 0)

            FundingKeypad(text: $model.amountText)

            FundingPrimaryButton(title: String(localized: .Funding.continueAction), isEnabled: model.canContinue) {
                model.continueFromAmount()
            }
        }
        .onChange(of: model.amountText) { _, _ in model.amountChanged() }
    }
}

private extension FundingAmountView {
    var title: String {
        if model.direction == .in {
            return String(localized: .Funding.amountAddTitle)
        }
        return String(localized: .Funding.amountWithdrawTitle)
    }

    var amountFigure: some View {
        DSAmount(
            amount: model.cash.figure(model.amount),
            symbol: model.cash.symbol,
            typography: .fundingFigure(digits: model.amountText.filter(\.isNumber).count)
        )
        .foregroundStyle(model.amount > 0 ? Color.fgPrimary : Color.fgTertiary)
        .lineLimit(1)
        .minimumScaleFactor(0.5)
        .animation(.snappy, value: model.amountText)
    }

    @ViewBuilder
    var captionLine: some View {
        if let issue = model.amountIssue {
            Text(message(for: issue))
                .typography(.captionMedium)
                .foregroundStyle(.fgError)
        } else if let minimum = model.learnedLimits.min {
            Text(minimumCaption(minimum))
                .typography(.captionMedium)
                .foregroundStyle(.fgSecondary)
        } else {
            Text(verbatim: " ")
                .typography(.captionMedium)
        }
    }

    var presets: some View {
        HStack(spacing: DSSpacings.small) {
            ForEach(Self.presets, id: \.self) { preset in
                Button {
                    model.amountText = NSDecimalNumber(decimal: preset).stringValue
                } label: {
                    DSAmount(amount: model.cash.figure(preset), symbol: model.cash.symbol, typography: .labelMedium)
                        .foregroundStyle(.fgPrimary)
                        .frame(maxWidth: .infinity)
                        .frame(height: 36)
                        .background(.bgSurfaceContainer, in: Capsule())
                }
                .buttonStyle(.plain)
            }
        }
    }

    func minimumCaption(_ minimum: Decimal) -> String {
        let label = model.cash.label(minimum)
        if model.direction == .in {
            return String(localized: .Funding.amountMinimum(amount: label))
        }
        return String(localized: .Funding.amountWithdrawMinimum(amount: label))
    }

    func message(for issue: FundingAmountIssue) -> String {
        switch issue {
        case let .belowMinimum(minimum):
            String(localized: .Funding.errorMinimum(amount: model.cash.label(minimum)))
        case let .aboveMaximum(maximum):
            String(localized: .Funding.errorMaximum(amount: model.cash.label(maximum)))
        case .notEnoughBalance:
            String(localized: .Funding.errorNotEnough(symbol: model.cash.symbol))
        }
    }
}

/// Crypto, Card and Bank, as pills. A rail no provider serves for this
/// direction is shown but cannot be picked.
struct FundingRailTabs: View {
    let selected: FundingRail
    let available: Set<FundingRail>
    let onSelect: (FundingRail) -> Void

    var body: some View {
        HStack(spacing: DSSpacings.small) {
            ForEach(FundingRail.displayOrder, id: \.self) { rail in
                tab(rail)
            }
        }
    }

    private func tab(_ rail: FundingRail) -> some View {
        let isSelected = rail == selected
        return Button {
            onSelect(rail)
        } label: {
            HStack(spacing: DSSpacings.extraSmall) {
                Image(systemName: rail.symbolName)
                    .font(.system(size: 12, weight: .semibold))
                Text(rail.title)
                    .typography(.labelMedium)
            }
            .foregroundStyle(isSelected ? Color.fgPrimaryInverted : Color.fgPrimary)
            .padding(.horizontal, DSSpacings.extraMedium)
            .frame(height: 32)
            .background(isSelected ? Color.bgActionPrimary : Color.bgSurfaceContainer, in: Capsule())
        }
        .buttonStyle(.plain)
        .disabled(!available.contains(rail))
        .opacity(available.contains(rail) ? 1 : 0.4)
    }
}

extension FundingRail {
    var title: String {
        switch self {
        case .crypto: String(localized: .Funding.railCrypto)
        case .card: String(localized: .Funding.railCard)
        case .bank: String(localized: .Funding.railBank)
        }
    }

    var symbolName: String {
        switch self {
        case .crypto: "bitcoinsign.circle.fill"
        case .card: "creditcard.fill"
        case .bank: "building.columns.fill"
        }
    }
}
