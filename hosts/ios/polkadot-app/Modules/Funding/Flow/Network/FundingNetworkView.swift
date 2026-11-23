import DesignSystem
import PolkadotUI
import SwiftUI

/// The networks the providers' crypto routes name, each closed to amounts
/// below the lowest minimum a provider has refused with there.
struct FundingNetworkView: View {
    let viewModel: FundingNetworkViewModel
    let presenter: FundingNetworkPresenterProtocol

    var body: some View {
        VStack(spacing: DSSpacings.medium) {
            FundingScreenHeader(title: String(localized: .Funding.networkTitle), onBack: presenter.back)

            if viewModel.rows.isEmpty {
                Text(.Funding.errorNoProvider)
                    .typography(.bodyMedium)
                    .foregroundStyle(.fgSecondary)
                    .padding(.top, DSSpacings.large)
            }

            ScrollView {
                VStack(spacing: DSSpacings.small) {
                    ForEach(viewModel.rows) { network in
                        row(network)
                    }
                }
            }
        }
        .fundingScreen()
    }

    private func row(_ network: FundingNetworkViewModel.Row) -> some View {
        let minimum = network.minimum

        return DisclosureListRow(title: network.mark.name, subtitle: minimum, isEnabled: minimum == nil) {
            FundingAssetMark(icon: network.mark.icon, letter: network.mark.monogram)
        } onTap: {
            presenter.chooseNetwork(network.id)
        }
        .fundingTile(isEnabled: minimum == nil)
    }
}

/// A network's or token's icon, or its first letter while there is none.
struct FundingAssetMark: View {
    let icon: UIImage?
    let letter: String
    var size: DSLetterAvatar.Size = .s40

    var body: some View {
        if let icon {
            Image(uiImage: icon)
                .resizable()
                .scaledToFit()
                .frame(width: size.dimension, height: size.dimension)
                .clipShape(Circle())
        } else {
            DSLetterAvatar(letter: letter, background: .fgTertiary, foreground: .white, size: size)
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
