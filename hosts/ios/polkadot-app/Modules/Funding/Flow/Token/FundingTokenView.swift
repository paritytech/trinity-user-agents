import DesignSystem
import PolkadotUI
import SwiftUI

/// The tokens the providers take on the chosen network.
struct FundingTokenView: View {
    let model: FundingScreenModel
    let presenter: FundingTokenPresenterProtocol

    var body: some View {
        VStack(spacing: DSSpacings.medium) {
            FundingScreenHeader(title: String(localized: .Funding.tokenTitle), onBack: presenter.back)

            if let network = model.network.map(FundingNetwork.init(id:)) {
                HStack {
                    Text(.Funding.tokenNetworkSelected)
                        .typography(.captionMedium)
                        .foregroundStyle(.fgSecondary)
                    Spacer()
                    Text(verbatim: network.name)
                        .typography(.captionMedium)
                        .foregroundStyle(.fgPrimary)
                    FundingAssetMark(logo: network.logo, letter: network.monogram, size: .s20)
                }
                .padding(.horizontal, DSSpacings.smallIncreased)
                .frame(height: 32)
                .background(.bgSurfaceMain, in: Capsule())
            }

            ScrollView {
                VStack(spacing: DSSpacings.small) {
                    ForEach(model.tokens) { token in
                        DisclosureListRow(title: token.symbol) {
                            FundingAssetMark(logo: token.logo, letter: token.monogram)
                        } onTap: {
                            presenter.chooseToken(token.symbol)
                        }
                        .fundingTile(isEnabled: true)
                    }
                }
            }
        }
    }
}
