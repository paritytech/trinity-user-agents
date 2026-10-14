import Testing
import Foundation
import SubstrateSdk

/// The core mints contact handles and this host recomputes them to answer a
/// lookup, so the two must hash identically. Pinned to the vector the core's
/// own test and the platform docs quote.
struct ContactHandleVectorTests {
    @Test("A handle is BLAKE2b-256 keyed with the handle key over the account")
    func matchesTheCoreVector() throws {
        let handleKey = Data(repeating: 0x11, count: 32)
        let account = Data(repeating: 0x22, count: 32)

        let handle = try account.blake2b32WithKey(handleKey)

        #expect(handle.toHex() == "d48c96fce9805f689b0bfa602feacdf3c7770d27e76c25d980eff0955e3714d2")
    }
}
