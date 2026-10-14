import Foundation
import Observation

public struct FiatOnRampQuickAmountViewModel: Identifiable {
    public let id: String
    public let value: Int
    public let title: String

    public init(id: String, value: Int, title: String) {
        self.id = id
        self.value = value
        self.title = title
    }
}

public protocol FiatOnRampViewModelProtocol: Observation.Observable {
    var amount: Int? { get set }
    var amountError: String? { get set }
    var quickAmounts: [FiatOnRampQuickAmountViewModel] { get set }
    var isContinueEnabled: Bool { get }
    var onAmountChanged: ((Int?) -> Void)? { get set }
    var onSelectQuickAmount: ((FiatOnRampQuickAmountViewModel) -> Void)? { get set }
    var onContinue: ((Int?) -> Void)? { get set }
}

@Observable
public final class FiatOnRampViewModel: FiatOnRampViewModelProtocol {
    public var amount: Int?
    public var amountError: String?
    public var quickAmounts: [FiatOnRampQuickAmountViewModel] = []
    public var isContinueEnabled: Bool {
        guard let amount else {
            return false
        }
        return amount > 0 && amountError == nil
    }

    public var onAmountChanged: ((Int?) -> Void)?
    public var onSelectQuickAmount: ((FiatOnRampQuickAmountViewModel) -> Void)?
    public var onContinue: ((Int?) -> Void)?

    public init() {}
}

public extension FiatOnRampViewModel {
    var bindableAmount: Int? {
        get { amount }
        set { onAmountChanged?(newValue) }
    }
}
