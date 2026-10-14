import SwiftUI
import DesignSystem

public struct IdentityDetailsCard: View {
    @State var viewModel: IdentityDetailsViewModelProtocol
    let isExpanded: Bool

    public init(viewModel: IdentityDetailsViewModelProtocol, isExpanded: Bool = false) {
        _viewModel = State(initialValue: viewModel)
        self.isExpanded = isExpanded
    }

    public var body: some View {
        compactCardView
    }

    private var compactCardView: some View {
        DSWalletCardContainer {
            PlasticCardView(viewModel: viewModel, isExpanded: isExpanded)
        }
        .cardAspectRatio()
    }
}

#Preview("Compact") {
    let vm = IdentityDetailsViewModel()
    vm.username = IdentityDetailsUsernameViewModel(value: "cyberpink.89", isClaimed: true)
    return ZStack {
        Color.black
        IdentityDetailsCard(viewModel: vm)
            .padding()
    }
}
