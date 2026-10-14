import SwiftUI
import PolkadotUI
import DesignSystem

struct IdentityQrSheetView: View {
    @State var viewModel: IdentityDetailsViewModelProtocol
    var onClose: () -> Void = {}

    init(viewModel: IdentityDetailsViewModelProtocol, onClose: @escaping () -> Void) {
        _viewModel = State(initialValue: viewModel)
        self.onClose = onClose
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(.identityCardScanQrTitle)
                    .typography(.headlineSmall)
                Spacer()
                Button(action: onClose) {
                    Image(.buttonClose)
                        .renderingMode(.template)
                }
                .buttonStyle(.dsIcon(style: .ghost, shape: .pill, size: .medium, glass: true))
            }
            .padding(.vertical, DSSpacings.small)
            .padding(.horizontal, DSSpacings.mediumIncreased)
            .padding(.top, DSSpacings.small)

            IdentityShareQrView(viewModel: viewModel)
        }
        .background(Color.bgSurfaceContainer)
        .clipShape(RoundedRectangle(cornerRadius: DSRadii.extraLarge))
    }
}
