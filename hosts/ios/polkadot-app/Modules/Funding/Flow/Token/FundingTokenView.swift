import DesignSystem
import PolkadotUI
import SwiftUI

/// The tokens the providers take on the chosen network.
struct FundingTokenView: View {
    let viewModel: FundingTokenViewModel
    let presenter: FundingTokenPresenterProtocol

    var body: some View {
        VStack(spacing: DSSpacings.medium) {
            FundingScreenHeader(title: String(localized: .Funding.tokenTitle), onBack: presenter.back)

            if let network = viewModel.network {
                HStack {
                    Text(.Funding.tokenNetworkSelected)
                        .typography(.captionMedium)
                        .foregroundStyle(.fgSecondary)
                    Spacer()
                    Text(verbatim: network.name)
                        .typography(.captionMedium)
                        .foregroundStyle(.fgPrimary)
                    FundingAssetMark(icon: network.icon, letter: network.monogram, size: .s20)
                }
                .padding(.horizontal, DSSpacings.smallIncreased)
                .frame(height: 32)
                .background(.bgSurfaceMain, in: Capsule())
            }

            ScrollView {
                VStack(spacing: DSSpacings.small) {
                    ForEach(viewModel.tokens) { token in
                        DisclosureListRow(title: token.name) {
                            FundingAssetMark(icon: token.icon, letter: token.monogram)
                        } onTap: {
                            presenter.chooseToken(token.id)
                        }
                        .fundingTile(isEnabled: true)
                    }
                }
            }
        }
        .fundingScreen()
    }
}
