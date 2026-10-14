import Coinage
import CoreData
import Foundation

/// The key index of a coin or voucher row, from its split `installationId` / `derivationIndex` columns.
/// `identifier` holds the same index as a string, but only as the row's key: nothing reads it back.
extension CDCoin {
    func keyIndex() throws -> CoinageKeyIndex {
        guard let installationHex = installationId else {
            throw CoreDataMapperError.missingRequiredData(keyPath: #keyPath(CDCoin.installationId))
        }
        return try CoinageKeyIndex(
            installation: CoinageInstallationId(hex: installationHex),
            item: UInt64(bitPattern: derivationIndex)
        )
    }
}

extension CDVoucher {
    func keyIndex() throws -> CoinageKeyIndex {
        guard let installationHex = installationId else {
            throw CoreDataMapperError.missingRequiredData(keyPath: #keyPath(CDVoucher.installationId))
        }
        return try CoinageKeyIndex(
            installation: CoinageInstallationId(hex: installationHex),
            item: UInt64(bitPattern: derivationIndex)
        )
    }
}
