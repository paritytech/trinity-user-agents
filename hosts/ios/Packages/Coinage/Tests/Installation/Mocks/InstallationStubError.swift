import AsyncExtensions
import BigInt
import DurableTransactions
import ExtrinsicService
import Foundation
import Individuality
import KeyDerivation
import os
import SubstrateSdk
@testable import Coinage

enum InstallationStubError: Error, Equatable {
    case unreachable
    case notEnoughPgas
    case nodeWentAway
}
