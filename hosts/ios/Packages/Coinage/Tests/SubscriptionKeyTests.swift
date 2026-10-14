import Foundation
import SubstrateSdk
import Testing
@testable import Coinage

@Suite("SubscriptionKey Tests")
struct SubscriptionKeyTests {
    @Test("Test member key mapping")
    func memberKeyMapping() {
        let publicKey = Data(repeating: 0xAB, count: 32)
        let key = SubscriptionKey.member(publicKey: publicKey)
        let mapping = key.mappingKey

        #expect(mapping == "m:\(publicKey.toHex())")
        #expect(SubscriptionKey(mappingKey: mapping) == key)
    }

    /// Ring state is subscribed per ring, not per voucher, so its keys carry the recycler.
    @Test("Test ringStatus key mapping")
    func ringStatusKeyMapping() {
        let key = SubscriptionKey.ringStatus(recycler: RecyclerKey(exponent: 3, index: 7))
        let mapping = key.mappingKey

        #expect(mapping == "rs:3:7")
        #expect(SubscriptionKey(mappingKey: mapping) == key)
    }

    @Test("Test unloadedCount key mapping")
    func unloadedCountKeyMapping() {
        let key = SubscriptionKey.unloadedCount(recycler: RecyclerKey(exponent: -2, index: 11))
        let mapping = key.mappingKey

        #expect(mapping == "uc:-2:11")
        #expect(SubscriptionKey(mappingKey: mapping) == key)
    }

    /// Two vouchers in one ring must produce one key, or the pipeline would ask the node for the
    /// same storage entry twice.
    @Test("Ring keys collapse for vouchers sharing a recycler")
    func ringKeysDeduplicate() {
        let ring = RecyclerKey(exponent: 4, index: 2)
        let keys = Set([
            SubscriptionKey.ringStatus(recycler: ring),
            SubscriptionKey.ringStatus(recycler: RecyclerKey(exponent: 4, index: 2))
        ])

        #expect(keys.count == 1)
    }

    @Test("Test invalid mapping keys return nil")
    func invalidMappingKeys() {
        #expect(SubscriptionKey(mappingKey: "x:123") == nil)
        #expect(SubscriptionKey(mappingKey: "m") == nil)
        #expect(SubscriptionKey(mappingKey: "m:zz") == nil)
        #expect(SubscriptionKey(mappingKey: "rs:7") == nil)
        #expect(SubscriptionKey(mappingKey: "rs:0:abc") == nil)
        #expect(SubscriptionKey(mappingKey: "uc:0") == nil)
    }
}
