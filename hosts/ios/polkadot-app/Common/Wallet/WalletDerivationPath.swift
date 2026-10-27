import Foundation
import KeyDerivation

/// Derivation paths of the built-in accounts. Product-account paths carry the 32-byte
/// index as a hex segment — always build them via `ProductDerivationPath`, never hand-format.
enum WalletDerivationPath {
    /// Main identity account `//product//uid.dot/{index_bytes(0)}`.
    static func main(for tld: String) throws -> String {
        try ProductDerivationPath.productAccount(
            productId: BuiltInProduct.uid(for: tld),
            index: DerivationIndex32(index: 0)
        )
    }

    /// Deposit account `//product//fund.dot/{index_bytes(0)}`.
    static func deposit(for tld: String) throws -> String {
        try ProductDerivationPath.productAccount(
            productId: BuiltInProduct.fund(for: tld),
            index: DerivationIndex32(index: 0)
        )
    }

    static var bulletInForChat: String { "//allowance//bulletin//chat" }
}
