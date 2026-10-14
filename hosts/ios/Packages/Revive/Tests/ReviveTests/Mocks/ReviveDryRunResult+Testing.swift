import BigInt
import Foundation
import SubstrateSdk
@testable import Revive

extension ReviveDryRunResult {
    static func completed(
        output: Data,
        reverted: Bool = false,
        weightRequired: Substrate.WeightV2 = Substrate.WeightV2(refTime: 11, proofSize: 22),
        storageDeposit: ReviveStorageDeposit = .charge(5)
    ) -> ReviveDryRunResult {
        ReviveDryRunResult(
            weightRequired: weightRequired,
            storageDeposit: storageDeposit,
            result: .success(
                ReviveExecResult(
                    flags: ReviveReturnFlags(bits: reverted ? ReviveExecResult.revertFlag : 0),
                    data: BytesCodable(wrappedValue: output)
                )
            )
        )
    }

    static func dispatchFailed(_ error: JSON) -> ReviveDryRunResult {
        ReviveDryRunResult(
            weightRequired: Substrate.WeightV2(refTime: 0, proofSize: 0),
            storageDeposit: .refund(0),
            result: .failure(error)
        )
    }
}
