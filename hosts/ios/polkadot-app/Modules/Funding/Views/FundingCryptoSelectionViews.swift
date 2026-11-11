import DesignSystem
import PolkadotUI
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

        return DisclosureListRow(title: network.name, subtitle: subtitle, isEnabled: minimum == nil) {
            FundingAssetMark(logo: network.logo, letter: network.monogram, tint: network.tint)
        } onTap: {
            model.chooseNetwork(network.id)
        }
        .fundingTile(isEnabled: minimum == nil)
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
                    FundingAssetMark(logo: network.logo, letter: network.monogram, tint: network.tint, size: .s20)
                }
                .padding(.horizontal, DSSpacings.smallIncreased)
                .frame(height: 32)
                .background(.bgSurfaceMain, in: Capsule())
            }

            ScrollView {
                VStack(spacing: DSSpacings.small) {
                    ForEach(model.tokens) { token in
                        DisclosureListRow(title: token.symbol) {
                            FundingAssetMark(logo: token.logo, letter: token.monogram, tint: token.tint)
                        } onTap: {
                            model.chooseToken(token.symbol)
                        }
                        .fundingTile(isEnabled: true)
                    }
                }
            }
        }
    }
}

/// A network's or token's mark: the app's logo where it has one, else its
/// first letter in its colour.
struct FundingAssetMark: View {
    let logo: ImageResource?
    let letter: String
    let tint: Color
    var size: DSLetterAvatar.Size = .s40

    var body: some View {
        if let logo {
            Image(logo)
                .resizable()
                .scaledToFit()
                .frame(width: size.dimension, height: size.dimension)
                .clipShape(Circle())
        } else {
            DSLetterAvatar(letter: letter, background: tint, foreground: .white, size: size)
        }
    }
}

extension View {
    /// The tile a selectable funding row sits on: filled while it can be
    /// chosen, outlined when it is closed.
    func fundingTile(isEnabled: Bool) -> some View {
        padding(.horizontal, DSSpacings.medium)
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
}
