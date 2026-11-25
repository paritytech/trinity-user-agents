import Foundation
import os
@testable import Coinage

/// Derives fixed placeholder coin key material — enough for callers that only need well-formed bytes.
/// Records the private-key derivations it serves, so a test can tell which coin a key was derived for.
final class StubCoinKeyFactory: CoinKeyDeriving {
    private let privateKeyIndices = OSAllocatedUnfairLock<[CoinageKeyIndex]>(initialState: [])

    var derivedPrivateKeyIndices: [CoinageKeyIndex] { privateKeyIndices.withLock { $0 } }

    func derivePublicKey(index _: CoinageKeyIndex) throws -> PublicKey { Data(repeating: 0, count: 32) }

    func derivePrivateKey(index: CoinageKeyIndex) throws -> PrivateKey {
        privateKeyIndices.withLock { $0.append(index) }
        return Data(repeating: 0, count: 64)
    }
}
