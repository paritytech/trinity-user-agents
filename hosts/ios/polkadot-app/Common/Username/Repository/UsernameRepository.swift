import Foundation
import Operation_iOS
import SubstrateSdk

protocol UsernameRepositoryProtocol {
    func queryUsernames(for accountIds: Set<AccountId>) -> CompoundOperationWrapper<AccountIdToUsername>
}
