import SwiftUI
import PolkadotUI
import DesignSystem

struct IdentityShareQrView: View {
    @State var viewModel: IdentityDetailsViewModelProtocol

    init(viewModel: IdentityDetailsViewModelProtocol) {
        _viewModel = State(initialValue: viewModel)
    }

    var body: some View {
        VStack(spacing: 0) {
            Text(.identityCardScanQrHint(PaymentAssetBranding.shared.current.symbol))
                .typography(.paragraphLarge)
                .foregroundStyle(Color.fgPrimary)
                .padding(.horizontal, DSSpacings.large)
                .padding(.vertical, DSSpacings.mediumIncreased)
                .fixedSize(horizontal: false, vertical: true)

            VStack(alignment: .center, spacing: DSSpacings.extraLarge) {
                if let username = viewModel.username {
                    HStack(spacing: 12) {
                        Text(verbatim: username.value)
                            .typography(.headlineSmall)
                            .foregroundStyle(.fgPrimary)

                        Button {
                            viewModel.onCopy?()
                        } label: {
                            Image(.iconCopy)
                                .renderingMode(.template)
                        }
                        .foregroundStyle(.fgPrimary)
                    }
                }

                Group {
                    if let qrCode = viewModel.qrCode {
                        qrCode
                            .renderingMode(.template)
                            .resizable()
                    } else {
                        Rectangle()
                            .foregroundStyle(.fgPrimary)
                    }
                }
                .foregroundStyle(.fgPrimary)
                .frame(width: 214, height: 214)
            }
            .frame(maxWidth: .infinity)
            .padding(.horizontal, DSSpacings.mediumIncreased)
            .padding(.vertical, DSSpacings.large)
            .background(Color.bgSurfaceNested)
            .clipShape(RoundedRectangle(cornerRadius: DSRadii.extraLarge))
            .padding(.horizontal, DSSpacings.mediumIncreased)
            .padding(.bottom, DSSpacings.mediumIncreased)
        }
        .background(Color.bgSurfaceContainer)
    }
}
