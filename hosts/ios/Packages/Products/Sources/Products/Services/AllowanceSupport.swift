import Foundation
import Individuality

public final class AllowanceSupport: @unchecked Sendable {
    public let allowancePromptRouter: AllowancePromptRouting
    public let sssManager: AllowanceManaging
    public let bulletInManager: AllowanceManaging
    public let smartContractManager: AllowanceManaging

    public init(
        allowancePromptRouter: AllowancePromptRouting,
        sssManager: AllowanceManaging,
        bulletInManager: AllowanceManaging,
        smartContractManager: AllowanceManaging
    ) {
        self.allowancePromptRouter = allowancePromptRouter
        self.sssManager = sssManager
        self.bulletInManager = bulletInManager
        self.smartContractManager = smartContractManager
    }
}
