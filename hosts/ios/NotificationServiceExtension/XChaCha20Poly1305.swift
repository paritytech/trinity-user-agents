import CryptoKit
import Foundation

/// XChaCha20-Poly1305, which CryptoKit does not have: HChaCha20 gives the subkey of the key and the first 16 bytes of
/// the 24-byte nonce, and ChaCha20-Poly1305 runs under the subkey with the last 8 bytes of the nonce.
enum XChaCha20Poly1305 {
    /// The plaintext of `ciphertext` (with its 16-byte tag at the end), or nil when the key, the nonce or the
    /// associated data does not match.
    static func open(_ ciphertext: Data, key: Data, nonce: Data, authenticating associatedData: Data) -> Data? {
        guard
            key.count == 32,
            nonce.count == 24,
            let chachaNonce = try? ChaChaPoly.Nonce(data: Data(count: 4) + nonce.suffix(8)),
            let box = try? ChaChaPoly.SealedBox(combined: Data(chachaNonce) + ciphertext)
        else {
            return nil
        }

        let subkey = HChaCha20.subkey(key: key, nonce: nonce.prefix(16))
        return try? ChaChaPoly.open(box, using: SymmetricKey(data: subkey), authenticating: associatedData)
    }
}

/// The HChaCha20 function of XChaCha20: the subkey that a 32-byte key and the first 16 bytes of a 24-byte nonce
/// give.
enum HChaCha20 {
    static func subkey(key: Data, nonce: Data) -> Data {
        var state: [UInt32] = [0x6170_7865, 0x3320_646E, 0x7962_2D32, 0x6B20_6574]
        state += words(key, count: 8)
        state += words(nonce, count: 4)

        for _ in 0 ..< 10 {
            quarterRound(&state, 0, 4, 8, 12)
            quarterRound(&state, 1, 5, 9, 13)
            quarterRound(&state, 2, 6, 10, 14)
            quarterRound(&state, 3, 7, 11, 15)
            quarterRound(&state, 0, 5, 10, 15)
            quarterRound(&state, 1, 6, 11, 12)
            quarterRound(&state, 2, 7, 8, 13)
            quarterRound(&state, 3, 4, 9, 14)
        }

        var subkey = Data(capacity: 32)
        for word in state[0 ..< 4] + state[12 ..< 16] {
            withUnsafeBytes(of: word.littleEndian) { subkey.append(contentsOf: $0) }
        }
        return subkey
    }

    private static func words(_ bytes: Data, count: Int) -> [UInt32] {
        let bytes = Array(bytes)
        return (0 ..< count).map { index in
            (0 ..< 4).reduce(UInt32(0)) { word, shift in
                word | UInt32(bytes[index * 4 + shift]) << (8 * shift)
            }
        }
    }

    private static func quarterRound(
        _ state: inout [UInt32],
        _ first: Int,
        _ second: Int,
        _ third: Int,
        _ fourth: Int
    ) {
        state[first] &+= state[second]
        state[fourth] = (state[fourth] ^ state[first]).rotatedLeft(16)
        state[third] &+= state[fourth]
        state[second] = (state[second] ^ state[third]).rotatedLeft(12)
        state[first] &+= state[second]
        state[fourth] = (state[fourth] ^ state[first]).rotatedLeft(8)
        state[third] &+= state[fourth]
        state[second] = (state[second] ^ state[third]).rotatedLeft(7)
    }
}

private extension UInt32 {
    func rotatedLeft(_ count: UInt32) -> UInt32 {
        self << count | self >> (32 - count)
    }
}
