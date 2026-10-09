import Coinage
import Foundation
import Keystore_iOS
import SubstrateSdk
import Testing
@testable import polkadot_app

struct IncomingPaymentKeychainSecretStoreTests {
    private let keychain = InMemoryKeychain()

    private func makeStore(ownerId: Data = Data(repeating: 7, count: 32)) -> IncomingPaymentKeychainSecretStore {
        IncomingPaymentKeychainSecretStore(keychain: keychain, logger: StubLogger(), ownerId: ownerId)
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
        let identifier = "topUpSource.v2." + Data(repeating: 7, count: 32).toHex() + ".top up:prod:p"
        try keychain.saveKey(Data("not a descriptor".utf8), with: identifier)

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
    @Test func sourceWritesAndCleanupAreOwnerScopedAndPreserveLegacySecrets() throws {
        let group = "top up:prod:shared"
        let first = makeStore()
        let secondOwner = Data(repeating: 8, count: 32)
        let second = makeStore(ownerId: secondOwner)
        let firstSource = IncomingPaymentSourceDescriptor.coins(secretKeys: [Data([1])])
        let secondSource = IncomingPaymentSourceDescriptor.coins(secretKeys: [Data([2])])
        let legacyData = try JSONEncoder().encode(IncomingPaymentSourceDescriptor.privateKey(secretKey: Data([3])))
        let legacyIdentifier = "topUpSource." + group
        try keychain.saveKey(legacyData, with: legacyIdentifier)

        // Neither owner may discover/adopt the old unscoped source before its own registration.
        #expect(try first.fetch(groupId: group) == nil)
        #expect(try second.fetch(groupId: group) == nil)
        try first.save(groupId: group, descriptor: firstSource)
        try second.save(groupId: group, descriptor: secondSource)
        #expect(try first.fetch(groupId: group) == firstSource)
        #expect(try second.fetch(groupId: group) == secondSource)

        // A replaced service's delayed cleanup cannot remove the replacement owner's source.
        first.remove(groupId: group)
        #expect(try first.fetch(groupId: group) == nil)
        #expect(try makeStore(ownerId: secondOwner).fetch(groupId: group) == secondSource)
        #expect(try keychain.fetchKey(for: legacyIdentifier) == legacyData)
    }
}
