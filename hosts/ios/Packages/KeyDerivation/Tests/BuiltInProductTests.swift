import KeyDerivation
import Testing
import TrUAPIHost

struct BuiltInProductTests {
    @Test(arguments: ["dot", "paseo", "testnet"])
    func reservedProductsUseTheConfiguredNetwork(tld: String) throws {
        #expect(try BuiltInProduct.dim2(for: tld) == ProductId("dim2.\(tld)"))
        #expect(try BuiltInProduct.personhood(for: tld) == ProductId("peopl.\(tld)"))
    }

    @Test(arguments: ["", "ksm", "localhost", "localhost:3000"])
    func invalidNetworkSuffixesThrow(tld: String) {
        #expect(throws: ProductIdError.self) { try BuiltInProduct.dim2(for: tld) }
        #expect(throws: ProductIdError.self) { try BuiltInProduct.personhood(for: tld) }
    }
}
