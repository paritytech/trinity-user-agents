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

enum TestContracts {
    static let contract = EvmAddress(repeating: 0x0C, count: EvmAddressFormat.size)
    static let otherContract = EvmAddress(repeating: 0x0D, count: EvmAddressFormat.size)
}
