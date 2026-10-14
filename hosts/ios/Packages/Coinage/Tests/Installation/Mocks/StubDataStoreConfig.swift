import AsyncExtensions
import BigInt
import DurableTransactions
import ExtrinsicService
import Foundation
import Individuality
import KeyDerivation
import os
import Revive
import SubstrateSdk
@testable import Coinage

final class StubDataStoreConfig: AccountDataStoreConfigProviding, @unchecked Sendable {
    private let address = OSAllocatedUnfairLock<Data?>(initialState: nil)

    init(contract: EvmAddress? = TestContracts.contract) {
        address.withLock { $0 = contract }
    }

    var contract: EvmAddress? {
        get { address.withLock { $0 } }
        set { address.withLock { $0 = newValue } }
    }

    func contractAddress() async -> EvmAddress? { contract }
}
