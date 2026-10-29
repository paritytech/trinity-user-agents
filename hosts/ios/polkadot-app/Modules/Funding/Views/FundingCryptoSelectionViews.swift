import DesignSystem
import SwiftUI

/// The networks the providers' crypto routes name, each closed to amounts
/// below the lowest minimum a provider has refused with there.
struct FundingNetworkView: View {
    let model: FundingFlowModel

    var body: some View {
        VStack(spacing: DSSpacings.medium) {
            FundingScreenHeader(title: String(localized: .Funding.networkTitle), onBack: model.back)

            if model.networks.isEmpty {
                Text(.Funding.errorNoProvider)
                    .typography(.bodyMedium)
                    .foregroundStyle(.fgSecondary)
                    .padding(.top, DSSpacings.large)
            }

            ScrollView {
                VStack(spacing: DSSpacings.small) {
                    ForEach(model.networks) { network in
                        row(network)
                    }
                }
            }
        }
    }

    private func row(_ network: FundingNetwork) -> some View {
        let minimum = model.minimum(network: network.id).flatMap { $0 > model.amount ? $0 : nil }
        let subtitle = minimum.map { minimum -> String in
            let label = model.cash.label(minimum)
            return String(localized: LocalizedStringResource.Funding.networkMinimum(amount: label))
        }

        return FundingSelectionRow(
            title: network.name,
            subtitle: subtitle,
            isEnabled: minimum == nil,
            mark: { FundingMonogram(text: network.monogram, tint: network.tint) },
            action: { model.chooseNetwork(network.id) }
        )
    }
}

/// The tokens the providers take on the chosen network.
struct FundingTokenView: View {
    let model: FundingFlowModel

    var body: some View {
        VStack(spacing: DSSpacings.medium) {
            FundingScreenHeader(title: String(localized: .Funding.tokenTitle), onBack: model.back)

            if let network = model.network.map(FundingNetwork.init(id:)) {
                HStack {
                    Text(.Funding.tokenNetworkSelected)
                        .typography(.captionMedium)
                        .foregroundStyle(.fgSecondary)
                    Spacer()
                    Text(verbatim: network.name)
                        .typography(.captionMedium)
                        .foregroundStyle(.fgPrimary)
                    FundingMonogram(text: network.monogram, tint: network.tint, size: 16)
                }
                .padding(.horizontal, DSSpacings.smallIncreased)
                .frame(height: 32)
                .background(.bgSurfaceMain, in: Capsule())
            }

            ScrollView {
                VStack(spacing: DSSpacings.small) {
                    ForEach(model.tokens) { token in
                        FundingSelectionRow(
                            title: token.symbol,
                            subtitle: nil,
                            isEnabled: true,
                            mark: { FundingMonogram(text: token.monogram, tint: token.tint) },
                            action: { model.chooseToken(token.symbol) }
                        )
                    }
                }
            }
        }
    }
}

/// A tile with a mark, a title, an optional reason line, and a chevron.
struct FundingSelectionRow<Mark: View>: View {
    let title: String
    let subtitle: String?
    let isEnabled: Bool
    @ViewBuilder let mark: () -> Mark
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: DSSpacings.smallIncreased) {
                mark()
                VStack(alignment: .leading, spacing: 2) {
                    Text(verbatim: title)
                        .typography(.bodyMediumEmphasized)
                        .foregroundStyle(isEnabled ? Color.fgPrimary : Color.fgSecondary)
                    if let subtitle {
                        Text(verbatim: subtitle)
                            .typography(.captionMedium)
                            .foregroundStyle(.fgTertiary)
                    }
                }
                Spacer()
                if isEnabled {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(.fgSecondary)
                }
            }
            .padding(.horizontal, DSSpacings.medium)
            .frame(minHeight: 60)
            .background(
                isEnabled ? Color.bgSurfaceNested : Color.clear,
                in: RoundedRectangle(cornerRadius: DSRadii.large)
            )
            .overlay {
                if !isEnabled {
                    RoundedRectangle(cornerRadius: DSRadii.large).stroke(Color.strokePrimary)
                }
            }
        }
        .buttonStyle(.plain)
        .disabled(!isEnabled)
    }
}
