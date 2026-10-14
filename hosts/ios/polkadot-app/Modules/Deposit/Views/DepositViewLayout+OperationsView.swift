import SwiftUI
import PolkadotUI

extension DepositViewLayout {
    private struct OperationView: View {
        let viewModel: DepositOperationViewModel

        func getTitleView() -> some View {
            switch viewModel.status {
            case .pendingSwap,
                 .inProgress:
                Text(.depositTransactionPending)
                    .textStyle(.body14Regular())
                    .foregroundStyle(.textAndIconsPrimaryDark)
            case .completed:
                Text(.depositTransactionComplete)
                    .textStyle(.body14Regular())
                    .foregroundStyle(.systemSuccess)
            case .failed:
                Text(.depositTransactionIncomplete)
                    .textStyle(.body14Regular())
                    .foregroundStyle(.systemError)
            }
        }

        func getAmount() -> AttributedString {
            var amountIn = AttributedString("\(viewModel.amountIn) ≈ ")
            amountIn.font = .body16Regular()
            amountIn.foregroundColor = .textAndIconsTertiaryDark

            var amountOut = AttributedString("\(viewModel.amountOut)")
            amountOut.font = .body16Regular()
            amountOut.foregroundColor = .textAndIconsPrimaryDark

            return amountIn + amountOut
        }

        @ViewBuilder
        func getStatusView() -> some View {
            switch viewModel.status {
            case let .pendingSwap(remained),
                 let .inProgress(remained):
                CountdownTimerView(
                    totalTime: remained,
                    size: CGSize(width: 40, height: 40)
                )
            case .completed:
                Image(.depositTransactionsSuccess)
            case .failed:
                Image(.depositTransactionsFailure)
            }
        }

        var body: some View {
            HStack(alignment: .center) {
                VStack(alignment: .leading, spacing: 4) {
                    getTitleView()
                    Text(getAmount())
                }

                Spacer()

                getStatusView()
            }
        }
    }

    struct OperationsView: View {
        let viewModel: [DepositOperationViewModel]

        func getTitleView(item: DepositOperationViewModel) -> some View {
            switch item.status {
            case .pendingSwap,
                 .inProgress:
                Text(.depositTransactionFundingPending)
                    .textStyle(.title18SemiBold())
                    .foregroundStyle(.textAndIconsPrimaryDark)
            case .completed:
                Text(.depositTransactionFundingComplete)
                    .textStyle(.title18SemiBold())
                    .foregroundStyle(.textAndIconsPrimaryDark)
            case .failed:
                Text(.depositTransactionFundingIncomplete)
                    .textStyle(.title18SemiBold())
                    .foregroundStyle(.textAndIconsPrimaryDark)
            }
        }

        @ViewBuilder
        func getResultView(item: DepositOperationViewModel) -> some View {
            switch item.status {
            case .pendingSwap,
                 .inProgress:
                HStack(alignment: .center, spacing: 0) {
                    Text(.depositTransactionPendingInfo)
                        .textStyle(.body14Regular())
                        .foregroundStyle(.depositInfoInProgress)
                        .multilineTextAlignment(.leading)

                    Spacer(minLength: 16)

                    Image(.depositTransactionsInfo)
                }
                .padding(16)
                .background(
                    .depositBackgroundInProgress,
                    in: RoundedRectangle(cornerRadius: 16)
                )
            case .completed:
                Text(.depositTransactionSuccessInfo)
                    .textStyle(.body14Regular())
                    .foregroundStyle(.systemSuccess)
                    .multilineTextAlignment(.leading)
                    .padding(16)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(
                        .systemBackgroundSuccess,
                        in: RoundedRectangle(cornerRadius: 16)
                    )
            case .failed:
                EmptyView()
            }
        }

        var body: some View {
            VStack(alignment: .leading, spacing: 0) {
                if let currentOp = viewModel.first {
                    getTitleView(item: currentOp)
                        .padding(.bottom, 24)
                }

                ForEach(Array(viewModel.enumerated()), id: \.element.id) { value in
                    OperationView(viewModel: value.element)

                    if value.offset != viewModel.count - 1 {
                        Divider()
                            .background(.textAndIconsSecondary)
                            .padding(.vertical, 12)
                    }
                }

                if let currentOp = viewModel.first {
                    getResultView(item: currentOp)
                        .padding(.top, 24)
                }
            }
            .padding(24)
            .frame(maxWidth: .infinity)
            .background(
                .white8,
                in: RoundedRectangle(cornerRadius: 24)
            )
        }
    }
}
