import SwiftUI
import PolkadotUI

extension DepositViewLayout {
    struct HeaderView: View {
        let viewModel: DepositAssetsViewModel

        var body: some View {
            VStack(alignment: .center, spacing: 12) {
                ViewThatFits(in: .horizontal) {
                    HStack(alignment: .center, spacing: 8) {
                        Text(.depositFundFrom)
                            .textStyle(.title32SemiBold())
                            .foregroundStyle(.textAndIconsPrimaryDark)

                        assetLabel
                    }

                    VStack(alignment: .center, spacing: 8) {
                        Text(.depositFundFrom)
                            .textStyle(.title32SemiBold())
                            .foregroundStyle(.textAndIconsPrimaryDark)

                        assetLabel
                    }
                }

                Text(
                    .depositDescription(
                        viewModel.assetName,
                        viewModel.network
                    )
                )
                .textStyle(.body16Regular())
                .multilineTextAlignment(.center)
                .foregroundStyle(.textAndIconsSecondary)
            }
        }

        var assetLabel: some View {
            // Can be replaced with Label
            HStack(alignment: .center, spacing: 3) {
                if let icon = viewModel.assetIcon {
                    AsyncImageView(
                        viewModel: icon,
                        settings: ImageViewModelSettings(targetSize: CGSize(width: 32, height: 32))
                    )
                    .frame(width: 32, height: 32)
                }

                Text(viewModel.assetName)
                    .textStyle(.title32SemiBold())
                    .foregroundStyle(Color(viewModel.assetColor))
            }
        }
    }
}
