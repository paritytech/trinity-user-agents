import DesignSystem
import PolkadotUI
import SwiftUI
import TrUAPIHost

/// The overlay's first screen: which rail, and how much.
struct FundingAmountView: View {
    let model: FundingScreenModel
    let presenter: FundingAmountPresenterProtocol

    private static let presets: [Decimal] = [10, 50, 100]

    var body: some View {
        VStack(spacing: DSSpacings.mediumIncreased) {
            FundingScreenHeader(title: title, leadingTitle: true, onBack: nil) {
                if model.historyClock != .hidden {
                    historyButton
                }
            }

            FundingRailTabs(selected: model.rail, available: model.availableRails) { rail in
                presenter.selectRail(rail)
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

            FundingKeypad(text: Binding(get: { model.amountText }, set: presenter.amountTextChanged))

            DSButton(.Funding.continueAction, expands: true) {
                presenter.continueFromAmount()
            }
            .disabled(!model.canContinue)
        }
    }
}

private extension FundingAmountView {
    /// Opens the history over the flow; the dot, ringed in the sheet's
    /// colour, says a session is still running.
    var historyButton: some View {
        Button(action: presenter.showHistory) {
            Image(systemName: "clock.arrow.circlepath")
                .font(.system(size: 16, weight: .semibold))
        }
        .buttonStyle(.dsIcon(style: .secondary, shape: .pill, size: .medium))
        .overlay(alignment: .bottomTrailing) {
            if model.historyClock == .running {
                Circle()
                    .fill(Color.fgError)
                    .frame(width: 8, height: 8)
                    .padding(2)
                    .background(Color.bgSurfaceContainer, in: Circle())
                    .offset(x: -3, y: -3)
            }
        }
        .accessibilityLabel(Text(.Funding.historyOpen))
    }

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
        } else {
            Text(minimumCaption(model.amountLimits.min))
                .typography(.captionMedium)
                .foregroundStyle(.fgSecondary)
        }
    }

    var presets: some View {
        HStack(spacing: DSSpacings.small) {
            ForEach(Self.presets, id: \.self) { preset in
                Button {
                    presenter.amountTextChanged(NSDecimalNumber(decimal: preset).stringValue)
                } label: {
                    DSAmount(amount: model.cash.figure(preset), symbol: model.cash.symbol, typography: .labelMedium)
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.ds(style: .tertiary, shape: .pill, size: .small))
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
            String(localized: .Funding.errorMinimum(amount: model.cash.label(units: minimum) ?? ""))
        case let .aboveMaximum(maximum):
            String(localized: .Funding.errorMaximum(amount: model.cash.label(units: maximum) ?? ""))
        case .notEnoughBalance:
            String(localized: .Funding.errorNotEnough(symbol: model.cash.symbol))
        }
    }
}

/// Crypto, Card and Bank, as pills with the rail's icon. A rail the core does
/// not offer for this direction is shown greyed and cannot be picked.
struct FundingRailTabs: View {
    private static let order: [FundingRail] = [.crypto, .card, .bank]
    private static let iconSize: CGFloat = 24

    let selected: FundingRail
    let available: Set<FundingRail>
    let onSelect: (FundingRail) -> Void

    var body: some View {
        HStack(spacing: DSSpacings.small) {
            ForEach(Self.order, id: \.self) { rail in
                tab(rail)
            }
        }
    }

    private func tab(_ rail: FundingRail) -> some View {
        let isSelected = rail == selected
        let isAvailable = available.contains(rail)
        return Button {
            onSelect(rail)
        } label: {
            HStack(spacing: DSSpacings.small) {
                Image(rail.icon)
                    .resizable()
                    .frame(width: Self.iconSize, height: Self.iconSize)
                    .clipShape(Circle())
                    .saturation(isAvailable ? 1 : 0)
                Text(rail.title)
            }
            // The web app's 40pt pill, the icon 8pt from its leading edge.
            .padding(.vertical, DSSpacings.extraTiny)
            .padding(.leading, -DSSpacings.tiny)
        }
        .buttonStyle(.ds(style: isSelected ? .primary : .tertiary, shape: .pill, size: .small))
        .disabled(!isAvailable)
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

    /// The rail's glyph, rasterised from the web app's icon, which draws
    /// with blurs no asset-catalog vector renders.
    var icon: ImageResource {
        switch self {
        case .crypto: .iconRailCrypto
        case .card: .iconRailCard
        case .bank: .iconRailBank
        }
    }
}
