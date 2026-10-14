import Foundation
import SubstrateSdk
import AssetsManagement

struct DepositExecLabel: Hashable, Codable {
    let chainAssetId: ChainAssetId
    let balance: Balance

    init(assetBalance: AssetBalance) {
        chainAssetId = assetBalance.chainAssetId
        balance = assetBalance.transferable
    }

    init(
        chainAssetId: ChainAssetId,
        balance: Balance
    ) {
        self.chainAssetId = chainAssetId
        self.balance = balance
    }
}

struct DepositExecutionItem: Equatable {
    enum Status: Equatable {
        case pendingSwap(expectedExecutionTime: TimeInterval)
        case inProgress(remainedTime: TimeInterval)
        case completed(receivedAmount: Balance)
        case failed
    }

    let execLabel: DepositExecLabel
    let amountIn: Balance
    let amountOut: Balance
    let status: Status

    init(execLabel: DepositExecLabel, amountIn: Balance, amountOut: Balance, status: Status) {
        self.execLabel = execLabel
        self.amountIn = amountIn
        self.amountOut = amountOut
        self.status = status
    }

    init(assetBalance: AssetBalance, amountIn: Balance, amountOut: Balance, status: Status) {
        execLabel = DepositExecLabel(assetBalance: assetBalance)
        self.amountIn = amountIn
        self.amountOut = amountOut
        self.status = status
    }

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

    var isFinished: Bool {
        switch status {
        case .pendingSwap,
             .inProgress:
            false
        case .completed,
             .failed:
            true
        }
    }

    func replacingStatus(_ newStatus: Status) -> DepositExecutionItem {
        DepositExecutionItem(
            execLabel: execLabel,
            amountIn: amountIn,
            amountOut: amountOut,
            status: newStatus
        )
    }
}
