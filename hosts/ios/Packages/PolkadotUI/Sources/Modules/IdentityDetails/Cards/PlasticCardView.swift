import SwiftUI
import DesignSystem
import ExternalAccessibility

struct PlasticCardView: View {
    @State var viewModel: IdentityDetailsViewModelProtocol
    let isExpanded: Bool

    init(viewModel: IdentityDetailsViewModelProtocol, isExpanded: Bool = false) {
        _viewModel = State(initialValue: viewModel)
        self.isExpanded = isExpanded
    }

    var body: some View {
        ZStack(alignment: .center) {
            cardBackground

            VStack(alignment: .leading, spacing: 0) {
                HStack(alignment: .center, spacing: 8) {
                    cardIcon

                    VStack(alignment: .leading, spacing: 4) {
                        if let username = viewModel.username {
                            Text(username.value)
                                .typography(.titleLarge)
                                .foregroundStyle(usernameColor)
                                .accessibilityId(AccessibilityID.Wallet.usernameDisplay)
                        }

                        if viewModel.isRankVisible {
                            rankView
                        }
                    }

                    Spacer()
                    qrButton
                        .frame(maxHeight: .infinity, alignment: .top)
                }
                .fixedSize(horizontal: false, vertical: true)

                Spacer()
            }
            .padding(.vertical, DSSpacings.extraMedium)
            .padding(.horizontal, DSSpacings.mediumIncreased)
        }
        .bordered(
            cornerRadius: 24,
            gradient: LinearGradient(
                colors: [Color.white, Color(hex: 0x99A1AC)],
                startPoint: .top,
                endPoint: .bottom
            )
        )
    }
}

private extension PlasticCardView {
    var qrButton: some View {
        Button {
            viewModel.onQrCode?()
        } label: {
            Image(.iconQrCode)
                .frame(width: 24, height: 24)
        }
        .padding(10)
        .disabled(isExpanded)
    }

    var cardIcon: some View {
        Image(.iconPlasticBasic)
            .resizable()
            .scaledToFit()
            .frame(width: 50, height: 50)
    }

    var rankView: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(.Identity.rankTitle)
                .typography(.bodySmall)
                .foregroundStyle(rankLabelColor)

            Text(.Identity.rankBasic)
                .typography(.titleSmall)
                .foregroundStyle(rankValueColor)
        }
    }

    var usernameColor: Color {
        Color(hex: 0x8C8F98)
    }

    var rankLabelColor: Color {
        Color(hex: 0x8C8F98)
    }

    var rankValueColor: Color {
        Color(hex: 0x8C8F98)
    }

    var cardBackground: some View {
        RadialGradient(
            colors: [Color(hex: 0xDEDFE3), Color(hex: 0xBBBCC0)],
            center: .center,
            startRadius: 0,
            endRadius: 200
        )
    }
}

#Preview("Card") {
    let vm = IdentityDetailsViewModel()
    vm.username = IdentityDetailsUsernameViewModel(value: "cyberpink.89", isClaimed: true)

    return ZStack {
        Color.black
        VStack {
            PlasticCardView(viewModel: vm)
                .cardAspectRatio()
                .padding()
        }
    }
}
