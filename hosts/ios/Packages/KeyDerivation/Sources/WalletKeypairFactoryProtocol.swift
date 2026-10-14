import Foundation
import NovaCrypto

public protocol WalletKeypairFactoryProtocol: Sendable {
    func derivePublicKey() throws -> IRPublicKeyProtocol
    func deriveKeypair() throws -> IRCryptoKeypairProtocol
}
