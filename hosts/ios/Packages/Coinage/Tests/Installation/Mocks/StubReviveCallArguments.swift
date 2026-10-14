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

struct StubReviveCallArguments: ReviveCallArgumentsProviding {
    var argument: RevivePallet.WeightLimitArgument = .weightLimit

    func weightLimitArgument() async throws -> RevivePallet.WeightLimitArgument { argument }
}
