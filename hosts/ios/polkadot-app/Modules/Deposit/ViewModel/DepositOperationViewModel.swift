import Foundation

struct DepositOperationViewModel: Identifiable {
    let id: String
    let amountIn: String
    let amountOut: String
    let status: DepositExecutionItem.Status

    var isInProgress: Bool {
        switch status {
        case .pendingSwap,
             .inProgress:
            true
        case .completed,
             .failed:
            false
        }
    }
}
