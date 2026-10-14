import Coinage
import CoreData
import Foundation
import SubstrateSdk

/// The read side of a registration/handoff transaction: the four public-key-keyed filters the
/// invariants check, bound to one `NSManagedObjectContext`.
///
/// Built by ``CoinageTxCoreDataRepository`` inside its `withTransaction` and handed to the caller's
/// validation closure, so nothing it reads can move before the write commits. Every asset is
/// identified by its on-chain public key — an own coin/voucher by its derived key, a received coin
/// by the key itself — so all four checks compare one key space.
struct CoinageTxValidationContext: CoinageTxValidationContextProtocol {
    let context: NSManagedObjectContext

    func filterMinted(_ keys: Set<PublicKey>) throws -> Set<PublicKey> {
        guard !keys.isEmpty else { return [] }
        let hexKeys = keys.map { $0.toHex() }
        let request = NSFetchRequest<CDCoinageTxOutput>(entityName: "CDCoinageTxOutput")
        request.predicate = NSPredicate(format: "coin.publicKey IN %@ OR voucher.publicKey IN %@", hexKeys, hexKeys)
        return try matched(context.fetch(request).map { [$0.coin?.publicKey, $0.voucher?.publicKey] }, in: keys)
    }

    func filterReceived(_ keys: Set<PublicKey>) throws -> Set<PublicKey> {
        guard !keys.isEmpty else { return [] }
        let request = NSFetchRequest<CDCoinageTxInput>(entityName: "CDCoinageTxInput")
        request.predicate = NSPredicate(
            format: "%K IN %@", #keyPath(CDCoinageTxInput.receivedPubKey), keys.map { $0.toHex() }
        )
        return try matched(context.fetch(request).map { [$0.receivedPubKey] }, in: keys)
    }

    func filterClaimed(_ keys: Set<PublicKey>) throws -> Set<PublicKey> {
        guard !keys.isEmpty else { return [] }
        let hexKeys = keys.map { $0.toHex() }
        let assetMatch = NSPredicate(
            format: "coin.publicKey IN %@ OR voucher.publicKey IN %@ OR %K IN %@",
            hexKeys,
            hexKeys,
            #keyPath(CDCoinageTxInput.receivedPubKey),
            hexKeys
        )
        let nonFailure = NSPredicate(
            format: "%K.%K != %d",
            #keyPath(CDCoinageTxInput.entry),
            #keyPath(CDDurableTx.status),
            CoinageTxStatus.failure.rawValue
        )
        let request = NSFetchRequest<CDCoinageTxInput>(entityName: "CDCoinageTxInput")
        request.predicate = NSCompoundPredicate(andPredicateWithSubpredicates: [assetMatch, nonFailure])
        return try matched(
            context.fetch(request).map { [$0.coin?.publicKey, $0.voucher?.publicKey, $0.receivedPubKey] },
            in: keys
        )
    }

    func filterHandedOff(_ keys: Set<PublicKey>) throws -> Set<PublicKey> {
        guard !keys.isEmpty else { return [] }
        let request = NSFetchRequest<CDCoin>(entityName: "CDCoin")
        request.predicate = NSPredicate(
            format: "publicKey IN %@ AND handoffMark != %d",
            keys.map { $0.toHex() },
            Int(CoinHandoffMark.none.rawValue)
        )
        return try matched(context.fetch(request).map { [$0.publicKey] }, in: keys)
    }
}

private extension CoinageTxValidationContext {
    /// The subset of `keys` present among the fetched rows' public-key hex strings (a row may carry
    /// several — a coin, a voucher, or a received key), so a matched row reports back the exact key.
    func matched(_ rowKeyHexes: [[String?]], in keys: Set<PublicKey>) throws -> Set<PublicKey> {
        var result: Set<PublicKey> = []
        for hexes in rowKeyHexes {
            for hex in hexes.compactMap({ $0 }) {
                let data = try Data(hexString: hex)
                if keys.contains(data) { result.insert(data) }
            }
        }
        return result
    }
}
