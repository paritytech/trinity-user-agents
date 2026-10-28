import KeyDerivation
import Testing

struct BuiltInProductTests {
    @Test func builtInFeaturesUseTheirReservedProductNames() throws {
        let products = [
            "game": try BuiltInProduct.dim2(for: "dot").rawValue,
            "personhood": try BuiltInProduct.personhood(for: "dot").rawValue
        ]

        #expect(products == ["game": "dim2.dot", "personhood": "peopl.dot"])
    }
}
