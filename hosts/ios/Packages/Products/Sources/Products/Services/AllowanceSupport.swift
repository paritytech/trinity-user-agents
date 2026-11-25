import Foundation
import Individuality
import SubstrateSdk

public final class AllowanceSupport: @unchecked Sendable {
    public let allowancePromptRouter: AllowancePromptRouting
    public let sssManager: any AllowanceManaging<StatementStoreAllowanceHolder>
    public let bulletInManager: any AllowanceManaging<AccountId>
    public let smartContractManager: any AllowanceManaging<AccountId>

    public init(
        allowancePromptRouter: AllowancePromptRouting,
        sssManager: any AllowanceManaging<StatementStoreAllowanceHolder>,
        bulletInManager: any AllowanceManaging<AccountId>,
        smartContractManager: any AllowanceManaging<AccountId>
    ) {
        self.allowancePromptRouter = allowancePromptRouter
        self.sssManager = sssManager
        self.bulletInManager = bulletInManager
        self.smartContractManager = smartContractManager
    }
}
