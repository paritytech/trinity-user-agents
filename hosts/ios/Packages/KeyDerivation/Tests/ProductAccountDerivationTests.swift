import Foundation
import KeyDerivation
import NovaCrypto
import SubstrateSdk
import Testing

/// Structural checks for the `//product//{productId}/{index}` path:
/// hard product firewall, soft hex-index junction, and the 64-byte AutoSigning secret.
@Suite("ProductAccountDerivation Tests")
struct ProductAccountDerivationTests {
    let mnemonic = "admit bounce found rally person winner script thing supreme honey credit goddess"

    // The whole "product accounts as derivation paths" design rests on this: the hex
    // path segment must decode to the exact 32 index bytes as a soft chain code, with
    // no substrate string normalization in between.
    @Test("Index path segment parses back to the exact 32-byte soft chain code")
    func indexSegmentParsesToRawChainCode() throws {
        let index = DerivationIndex32(index: 5)

        let path = try ProductDerivationPath.productAccount(productId: "browse.dot", index: index)
        let chaincodes = try SubstrateJunctionFactory().parse(path: path).chaincodes

        #expect(chaincodes.count == 3)
        #expect(chaincodes[0].type == .hard)
        #expect(chaincodes[1].type == .hard)
        #expect(chaincodes[2] == Chaincode(data: index.bytes, type: .soft))
    }

    @Test("Product account path is the product root plus the index segment")
    func pathStructure() throws {
        let index = DerivationIndex32(index: 5)

        let root = try ProductDerivationPath.productRoot(productId: "browse.dot")
        let account = try ProductDerivationPath.productAccount(productId: "browse.dot", index: index)

        #expect(root == "//product//browse.dot")
        #expect(account == "\(root)/\(index.asPathSegment())")
    }

    @Test("Built-in variant formats the same path as the validated one")
    func builtInVariantMatchesValidated() throws {
        let validated = try ProductDerivationPath.productAccount(
            productId: "browse.dot",
            index: DerivationIndex32(index: 5)
        )

        #expect(ProductDerivationPath.builtInAccount("browse.dot", index: 5) == validated)
    }

    @Test("Child account is soft-derivable from the product root keypair")
    func childIsSoftDerivableFromSubtree() throws {
        let index = DerivationIndex32(index: 0)
        let fullPath = try ProductDerivationPath.productAccount(productId: "browse.dot", index: index)
        let rootPath = try ProductDerivationPath.productRoot(productId: "browse.dot")

        let fromRoot = try makeKeypair(path: fullPath)
        let subtree = try makeKeypair(path: rootPath)
        let fromSubtree = try SR25519KeypairFactory().deriveChildKeypairFromParent(
            subtree,
            chaincodeList: [Chaincode(data: index.bytes, type: .soft)]
        )

        #expect(fromRoot.publicKey().rawData() == fromSubtree.publicKey().rawData())
    }

    @Test("Product root secret is the 64-byte expanded sr25519 key")
    func productRootSecretIsExpanded() throws {
        let rootPath = try ProductDerivationPath.productRoot(productId: "browse.dot")

        let secret = try makeKeypair(path: rootPath).privateKey().rawData()

        #expect(secret.count == 64)
    }

    @Test("Different products derive under different hard junctions")
    func productsAreFirewalled() throws {
        let index = DerivationIndex32(index: 0)
        let first = try makeKeypair(
            path: ProductDerivationPath.productAccount(productId: "a.dot", index: index)
        )
        let second = try makeKeypair(
            path: ProductDerivationPath.productAccount(productId: "b.dot", index: index)
        )

        #expect(first.publicKey().rawData() != second.publicKey().rawData())
    }

    @Test("Product ids containing junction separators are rejected")
    func invalidProductIdsThrow() throws {
        for productId in ["", "a/b", "a//b"] {
            #expect(throws: ProductDerivationPathError.invalidProductId(productId)) {
                _ = try ProductDerivationPath.productRoot(productId: productId)
            }
        }
    }

    private func makeKeypair(path: String) throws -> IRCryptoKeypairProtocol {
        // Mirrors WalletMnemonicKeypairFactory's seed handling for a fixed mnemonic.
        let chaincodes = try SubstrateJunctionFactory().parse(path: path).chaincodes
        let seedResult = try SeedFactory().deriveSeed(from: mnemonic, password: "")
        return try SR25519KeypairFactory().createKeypairFromSeed(
            seedResult.seed.miniSeed,
            chaincodeList: chaincodes
        )
    }
}
