import CryptoKit
import Foundation
import os
import StatementStore

final class RecordingStatementSignerManager: StatementStoreSignerManaging, @unchecked Sendable {
    private let keyIds = OSAllocatedUnfairLock<[String]>(initialState: [])

    var requestedKeyIds: [String] { keyIds.withLock { $0 } }

    static func accountId(for keyId: String) -> Data {
        Data(SHA256.hash(data: Data(keyId.utf8)))
    }

    func makeSigner(for signerKeyId: String) throws -> StatementStoreSigning {
        keyIds.withLock { $0.append(signerKeyId) }

        return StubStatementSigner(accountId: Self.accountId(for: signerKeyId))
    }
}
