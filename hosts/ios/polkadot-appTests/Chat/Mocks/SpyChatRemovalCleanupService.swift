import Foundation
import os
import SubstrateSdk
@testable import polkadot_app

final class SpyChatRemovalCleanupService: ChatRemovalCleanupServicing {
    private let removedAccountIds = OSAllocatedUnfairLock<[AccountId]>(initialState: [])

    var accountIds: [AccountId] { removedAccountIds.withLock { $0 } }

    func cleanUpAfterRemoval(of accountId: AccountId) async throws {
        removedAccountIds.withLock { $0.append(accountId) }
    }
}
