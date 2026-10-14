import SwiftUI
import PolkadotUI

struct DepositViewLayout: View {
    @State var viewModel = DepositViewModel()

    var body: some View {
        ScrollView(.vertical) {
            VStack(alignment: .center) {
                if let assetsViewModel = viewModel.assetsViewModel {
                    HeaderView(viewModel: assetsViewModel)
                        .padding(.bottom, 24)
                }

                if
                    let operationsViewModel = viewModel.operationsViewModel,
                    !operationsViewModel.isEmpty {
                    OperationsView(viewModel: operationsViewModel)
                        .padding(.bottom, 8)
                }

                if let summaryViewModel = viewModel.summaryViewModel {
                    SummaryView(
                        viewModel: summaryViewModel,
                        onCopyAddress: viewModel.onCopyAddress
                    )
                }

                if viewModel.isLoading {
                    ProgressView()
                        .scaleEffect(1.3)
                        .tint(.white100)
                }
            }
        }
        .contentMargins(.horizontal, 16)
        .contentMargins(.top, 8)
    }
}
