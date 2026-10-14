import Observation
import SwiftUI

protocol DepositViewModelProtocol {
    var assetsViewModel: DepositAssetsViewModel? { get set }
    var operationsViewModel: [DepositOperationViewModel]? { get set }
    var summaryViewModel: DepositSummaryViewModel? { get set }

    var onCopyAddress: (() -> Void)? { get set }
}

@Observable
class DepositViewModel: DepositViewModelProtocol {
    var assetsViewModel: DepositAssetsViewModel?
    var operationsViewModel: [DepositOperationViewModel]?
    var summaryViewModel: DepositSummaryViewModel?

    var onCopyAddress: (() -> Void)?

    var isLoading: Bool {
        summaryViewModel == nil || operationsViewModel == nil
    }
}
