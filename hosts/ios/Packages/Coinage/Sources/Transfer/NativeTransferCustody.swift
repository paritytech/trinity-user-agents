import Foundation

/// Durable native custody contains derivation coordinates, never private keys or a serialized memo.
public struct NativeTransferCustody: Codable, Equatable, Sendable {
    public struct Entry: Codable, Equatable, Sendable {
        public let coinDerivationIndex: DerivationIndex
        public let valueExponent: Int16
        public let publicKey: PublicKey

        public init(coinDerivationIndex: DerivationIndex, valueExponent: Int16, publicKey: PublicKey) {
            self.coinDerivationIndex = coinDerivationIndex
            self.valueExponent = valueExponent
            self.publicKey = publicKey
        }
    }

    public let custodyId: String
    public let entries: [Entry]

    public init(custodyId: String, entries: [Entry]) {
        self.custodyId = custodyId
        self.entries = entries
    }

    init(custodyId: String, coins: [Coin]) {
        self.init(custodyId: custodyId, entries: coins.map {
            Entry(coinDerivationIndex: $0.derivationIndex, valueExponent: $0.exponent, publicKey: $0.publicKey)
        })
    }

    public var assets: [OwnAsset] {
        entries.map { .coin($0.coinDerivationIndex, $0.publicKey) }
    }

    var memoEntries: [PlannedMemoEntry] {
        entries.map { PlannedMemoEntry(coinDerivationIndex: $0.coinDerivationIndex, valueExponent: $0.valueExponent) }
    }

    /// Run inside the registration transaction, before writing either asset rows or custody marks.
    public func validate(
        registrations: [CoinageAssetRegistration],
        transaction: any CoinageTxValidationContextProtocol
    ) throws {
        let keys = Set(entries.map(\.publicKey))
        guard !custodyId.isEmpty, !entries.isEmpty,
              keys.count == entries.count,
              Set(entries.map(\.coinDerivationIndex)).count == entries.count else {
            throw NativeTransferCustodyError.invalidRecord
        }
        let consumed = Set(registrations.flatMap { $0.inputs.map(\.publicKey) })
        guard keys.isDisjoint(with: consumed) else {
            throw NativeTransferCustodyError.assetUnavailable
        }
        try CoinageTxRegistrationValidator().validateHandoff(keys, transaction: transaction)
    }
}

public enum NativeTransferCustodyError: Error, Equatable {
    case alreadyRegistered
    case invalidRecord
    /// A record exists but its durable transaction or committed coin marks are missing. Never retry a spend.
    case incompleteRegistration
    case assetUnavailable
    case amountMismatch
}
