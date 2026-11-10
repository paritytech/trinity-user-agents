@testable import polkadot_app
import Testing

struct NetworkSuffixTldReaderTests {
    @Test(arguments: ["paseo", "dot", "w3s-dev", "a1"])
    func acceptsBareLabel(_ value: String) {
        #expect(NetworkSuffixTldReader.isBareLabel(value))
    }

    @Test(arguments: [".paseo", "paseo.dot", "", "-paseo", "Paseo", "pas eo", String(repeating: "a", count: 64)])
    func rejectsAnythingButBareLabel(_ value: String) {
        #expect(!NetworkSuffixTldReader.isBareLabel(value))
    }
}
