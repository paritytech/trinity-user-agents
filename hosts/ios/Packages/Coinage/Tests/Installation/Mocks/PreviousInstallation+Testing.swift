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

extension PreviousInstallation {
    func changing(
        coinScanNextIndex: DerivationIndex? = nil,
        voucherScanNextIndex: DerivationIndex? = nil,
        initialScanCompleted: Bool? = nil,
        isUserConfirmedCompletion: Bool? = nil
    ) -> PreviousInstallation {
        PreviousInstallation(
            id: id,
            coinScanNextIndex: coinScanNextIndex ?? self.coinScanNextIndex,
            voucherScanNextIndex: voucherScanNextIndex ?? self.voucherScanNextIndex,
            initialScanCompleted: initialScanCompleted ?? self.initialScanCompleted,
            isUserConfirmedCompletion: isUserConfirmedCompletion ?? self.isUserConfirmedCompletion
        )
    }
}
