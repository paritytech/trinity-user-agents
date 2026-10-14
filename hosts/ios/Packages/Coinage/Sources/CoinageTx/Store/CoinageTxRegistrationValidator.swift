import Foundation
import SubstrateSdk

/// Validates registration invariants for a whole batch directly against the store transaction, keyed by
/// public key.
///
/// Every asset already carries its on-chain public key, so the four checks compare one key space,
/// matching the store's ``CoinageTxValidationContextProtocol``. Within-batch conflicts are caught here
/// too, since the rows do not exist yet when the batch is validated. The offending key is reported.
public struct CoinageTxRegistrationValidator {
    public init() {}

    /// Validates the registration invariants across `registrations`, in order:
    /// 1. Non-empty entry (consumes or mints something)
    /// 2. Fresh outputs — not minted by another entry, not a key received from a peer, unique in batch
    /// 3. Blocked handoff — no input carrying a handoff mark
    /// 4. Unique consumer — no input already claimed by a non-failure entry, unique in batch
    public func validate(
        _ registrations: [CoinageAssetRegistration],
        transaction: CoinageTxValidationContextProtocol
    ) throws {
        guard registrations.allSatisfy({ !$0.isEmpty }) else {
            throw CoinageTxError.emptyEntry
        }

        let outputKeys = registrations.flatMap { $0.outputs.map(\.publicKey) }
        let inputKeys = registrations.flatMap { $0.inputs.map(\.publicKey) }

        let notFresh = try transaction.filterMinted(Set(outputKeys))
            .union(transaction.filterReceived(Set(outputKeys)))
            .union(outputKeys.duplicateKeys())
        if let key = outputKeys.first(where: { notFresh.contains($0) }) {
            throw CoinageTxError.outputNotFresh(key.toHex())
        }

        let handedOff = try transaction.filterHandedOff(Set(inputKeys))
        if let key = inputKeys.first(where: { handedOff.contains($0) }) {
            throw CoinageTxError.inputHandedOff(key.toHex())
        }

        let claimed = try transaction.filterClaimed(Set(inputKeys)).union(inputKeys.duplicateKeys())
        if let key = inputKeys.first(where: { claimed.contains($0) }) {
            throw CoinageTxError.inputAlreadyClaimed(key.toHex())
        }
    }
}

private extension [PublicKey] {
    /// Keys appearing more than once — a within-batch collision the store cannot yet see.
    func duplicateKeys() -> Set<PublicKey> {
        var seen: Set<PublicKey> = []
        var duplicates: Set<PublicKey> = []
        for key in self where !seen.insert(key).inserted {
            duplicates.insert(key)
        }
        return duplicates
    }
}
