import Coinage
import CoreData
import DurableTransactions
import Foundation
import Operation_iOS
import SubstrateSdk

/// Maps a `CDDurableTx` row joined to its coinage input/output rows to ``CoinageTxEntry``.
///
/// Inputs and outputs are immutable: they are written once when the entry is registered (by
/// ``CoinageAssetLedgerCoreData`` inside the engine's transaction) and never rewritten. Each row
/// references its asset through the `CDCoin` / `CDVoucher` relation — or `receivedPubKey` for a coin
/// received from a peer — which must already exist at registration. The mapper only reads.
final class CoinageTxEntryMapper: CoreDataMapperProtocol {
    typealias DataProviderModel = CoinageTxEntry
    typealias CoreDataEntity = CDDurableTx

    private let durableMapper = DurableTxMapper()

    var entityIdentifierFieldName: String { #keyPath(CDDurableTx.identifier) }

    func transform(entity: CDDurableTx) throws -> CoinageTxEntry {
        try CoinageTxEntry(
            entry: durableMapper.transform(entity: entity),
            inputs: CoinageTxAssetRows.transformInputs(from: entity.inputs),
            outputs: CoinageTxAssetRows.transformOutputs(from: entity.outputs)
        )
    }

    /// Read-only by design: the engine row is written by `DurableTxCoreDataRepository` and the asset rows
    /// by `CoinageAssetLedgerCoreData` inside its registration scope. A second write path here would
    /// bypass the sequence assignment and the invariants.
    func populate(entity _: CDDurableTx, from _: CoinageTxEntry, using _: NSManagedObjectContext) throws {
        throw CoreDataMapperError.unsupported
    }
}

/// Signals coinage's coin and voucher rows when an engine status write lands, so their CoreData
/// snapshot subscribers re-emit — the durability overlay stays current without a separate change-merge.
struct CoinageTxRowObserver: DurableTxRowObserving {
    func didChangeStatus(of entity: CDDurableTx, in _: NSManagedObjectContext) {
        CoinageTxAssetRows.touchRelatedAssets(of: entity)
    }
}

/// Coinage's input/output rows on a `CDDurableTx`: reading them back, writing them at registration,
/// and signalling the assets they link to.
enum CoinageTxAssetRows {
    static func transformInputs(from rows: NSSet?) throws -> [CoinageTxInput] {
        guard let rows = rows as? Set<CDCoinageTxInput> else { return [] }
        return try rows.compactMap { row in
            if let hex = row.receivedPubKey {
                return try .coin(.received(Data(hexString: hex)))
            }
            if let coin = row.coin {
                return try .coin(.own(coin.keyIndex(), publicKey(coin.publicKey)))
            }
            if let voucher = row.voucher {
                return try .recyclerVoucher(voucher.keyIndex(), publicKey(voucher.publicKey))
            }
            return nil
        }
    }

    static func transformOutputs(from rows: NSSet?) throws -> [OwnAsset] {
        guard let rows = rows as? Set<CDCoinageTxOutput> else { return [] }
        return try rows.compactMap { row in
            if let coin = row.coin {
                return try .coin(coin.keyIndex(), publicKey(coin.publicKey))
            }
            if let voucher = row.voucher {
                return try .recyclerVoucher(voucher.keyIndex(), publicKey(voucher.publicKey))
            }
            return nil
        }
    }

    static func populate(
        entity: CDDurableTx,
        inputs: [CoinageTxInput],
        outputs: [OwnAsset],
        using context: NSManagedObjectContext
    ) throws {
        try populateInputs(entity: entity, inputs: inputs, using: context)
        try populateOutputs(entity: entity, outputs: outputs, using: context)
    }

    /// Signals the linked coins/vouchers as changed so their CoreData snapshot subscribers re-emit when
    /// this entry's status changes — the `willChange`/`didChange` TouchParent pattern.
    static func touchRelatedAssets(of entity: CDDurableTx) {
        for row in (entity.inputs as? Set<CDCoinageTxInput>) ?? [] {
            if let coin = row.coin {
                touch(coin, key: #keyPath(CDCoin.coinageTxInputs))
            }
            if let voucher = row.voucher {
                touch(voucher, key: #keyPath(CDVoucher.coinageTxInputs))
            }
        }

        for row in (entity.outputs as? Set<CDCoinageTxOutput>) ?? [] {
            if let coin = row.coin {
                touch(coin, key: #keyPath(CDCoin.coinageTxOutput))
            }
            if let voucher = row.voucher {
                touch(voucher, key: #keyPath(CDVoucher.coinageTxOutput))
            }
        }
    }
}

private extension CoinageTxAssetRows {
    /// The stored on-chain public key of a linked coin/voucher row. Persisted at mint (coinage.md #1), so
    /// the entry never derives it on the fly.
    static func publicKey(_ hex: String?) throws -> PublicKey {
        guard let hex else {
            throw CoreDataMapperError.missingRequiredData(keyPath: #keyPath(CDCoin.publicKey))
        }
        return try Data(hexString: hex)
    }

    static func populateInputs(
        entity: CDDurableTx,
        inputs: [CoinageTxInput],
        using context: NSManagedObjectContext
    ) throws {
        for input in inputs {
            let row = try context.insertNew(CDCoinageTxInput.self)
            row.entry = entity

            switch input {
            case let .coin(coinInput):
                switch coinInput {
                case .own:
                    guard let coin = CoinageTxAssetLinker.coin(for: input, in: context) else {
                        throw CoreDataMapperError.missingRequiredData(keyPath: #keyPath(CDCoinageTxInput.coin))
                    }
                    row.coin = coin
                case let .received(accountId):
                    row.receivedPubKey = accountId.toHex()
                }
            case .recyclerVoucher:
                guard let voucher = CoinageTxAssetLinker.voucher(for: input, in: context) else {
                    throw CoreDataMapperError.missingRequiredData(keyPath: #keyPath(CDCoinageTxInput.voucher))
                }
                row.voucher = voucher
            }
        }
    }

    static func populateOutputs(
        entity: CDDurableTx,
        outputs: [OwnAsset],
        using context: NSManagedObjectContext
    ) throws {
        for output in outputs {
            let row = try context.insertNew(CDCoinageTxOutput.self)
            row.entry = entity

            switch output {
            case .coin:
                guard let coin = CoinageTxAssetLinker.coin(for: output, in: context) else {
                    throw CoreDataMapperError.missingRequiredData(keyPath: #keyPath(CDCoinageTxOutput.coin))
                }
                row.coin = coin
            case .recyclerVoucher:
                guard let voucher = CoinageTxAssetLinker.voucher(for: output, in: context) else {
                    throw CoreDataMapperError.missingRequiredData(keyPath: #keyPath(CDCoinageTxOutput.voucher))
                }
                row.voucher = voucher
            }
        }
    }

    static func touch(_ object: NSManagedObject, key: String) {
        object.willChangeValue(forKey: key)
        object.didChangeValue(forKey: key)
    }
}
