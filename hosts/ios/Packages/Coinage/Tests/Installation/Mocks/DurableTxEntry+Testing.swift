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

extension DurableTxEntry {
    static func registration(
        _ target: InstallationRegistrationTarget,
        id: DurableTxId = UUID(),
        status: DurableTxStatus
    ) -> DurableTxEntry {
        .fixture(id: id, domainId: .coinageInstallation, status: status, groupId: target.registrationGroup)
    }

    /// The production copy helper: it carries the attempt across, so a test cannot accidentally
    /// rebuild an entry with a different one.
    func changing(status: DurableTxStatus) -> DurableTxEntry {
        withStatus(status)
    }
}
