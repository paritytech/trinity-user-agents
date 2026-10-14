import Coinage
import Foundation
import Keystore_iOS
import Testing
@testable import polkadot_app

struct IncomingPaymentKeychainSecretStoreTests {
    private let keychain = InMemoryKeychain()

    private func makeStore() -> IncomingPaymentKeychainSecretStore {
        IncomingPaymentKeychainSecretStore(keychain: keychain, logger: StubLogger())
    }

    @Test func savedDescriptorReadsBack() throws {
        let store = makeStore()
        let descriptor = IncomingPaymentSourceDescriptor.coins(secretKeys: [Data([0x01]), Data([0x02])])

        try store.save(groupId: "top up:prod:p", descriptor: descriptor)

        #expect(try store.fetch(groupId: "top up:prod:p") == descriptor)
    }

    @Test func missingEntryReadsAsNil() throws {
        #expect(try makeStore().fetch(groupId: "top up:prod:none") == nil)
    }

    @Test func entryThatDoesNotDecodeIsCorruptedNotUnreadable() throws {
        let store = makeStore()
        try keychain.saveKey(Data("not a descriptor".utf8), with: "topUpSource.top up:prod:p")

        #expect(throws: IncomingPaymentSecretStoreError.corrupted) {
            _ = try store.fetch(groupId: "top up:prod:p")
        }
    }

    @Test func removeDropsTheEntryAndTolerateAMissingOne() throws {
        let store = makeStore()
        try store.save(groupId: "top up:prod:p", descriptor: .privateKey(secretKey: Data([0x09])))

        store.remove(groupId: "top up:prod:p")
        store.remove(groupId: "top up:prod:p")

        #expect(try store.fetch(groupId: "top up:prod:p") == nil)
    }
}
