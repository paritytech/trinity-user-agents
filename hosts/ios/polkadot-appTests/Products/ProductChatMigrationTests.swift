import Foundation
import Keystore_iOS
import SubstrateSdk
import Testing
import TrUAPIHost
@testable import polkadot_app

struct ProductChatMigrationTests {
    private struct StagedExport: NativeChatHistoryExporting {
        let history: Data?

        func export() async throws -> Data? {
            history
        }
    }

    private final class Imports: @unchecked Sendable {
        private let lock = NSLock()
        private var keys: [Data] = []

        func record(_ key: Data) -> Bool {
            lock.withLock { keys.append(key) }
            return true
        }

        var all: [Data] {
            lock.withLock { keys }
        }
    }

    // `HistoryExportCodec.enc` of the chat product for the same state: the product imports what iOS writes.
    private static let productEncoding = "0004" + String(repeating: "b0", count: 32) + "0118626f622e3031"
        + String(repeating: "b1", count: 32) + "010c0102030101000100e8030000000000000004"
        + String(repeating: "d1", count: 32) + String(repeating: "d2", count: 32) + "04147265712d31"
        + String(repeating: "b0", count: 32) + "d0070000000000000000000004" + String(repeating: "b0", count: 32)
        + "40086d31b80b00000000000000000868690001b80b000000000000"

    @Test func theExportIsTheBytesTheChatProductImports() throws {
        let bob = Data(repeating: 0xB0, count: 32)
        let request = Chat.Request(
            requestId: "req-1",
            contactAccountId: bob,
            timestamp: 2000,
            status: .incoming(.new),
            message: nil
        )
        var contact = Chat.Contact(
            accountId: bob,
            username: "bob.01",
            publicKey: Data(repeating: 0xB1, count: 32),
            ownKeyId: .init(signKeyId: "sign", encryptionKeyId: "encrypt"),
            isBlocked: true,
            devices: [.init(
                statementAccountId: Data(repeating: 0xD1, count: 32),
                encryptionPublicKey: Data(repeating: 0xD2, count: 32)
            )],
            pendingDevicesFanOut: false,
            addedAt: Date(timeIntervalSince1970: 1),
            acceptedAt: nil
        )
        contact.pushToken = Data([1, 2, 3])
        contact.peerPlatform = .ios
        contact.chatRequest = request
        let message = Chat.LocalMessage(
            messageId: "m1",
            chatId: .person(bob),
            origin: .user,
            creationSource: .localDevice,
            status: .outgoing(.sent),
            timestamp: 3000,
            content: .text("hi"),
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )

        let encoded = try ProductChatHistoryExport(contacts: [contact], messages: [message]).encoded()

        #expect(encoded.toHex() == Self.productEncoding)
    }

    @Test func theHistoryIsStagedOnceReadInPartsAndTheDeviceKeyGoesToTheCore() async throws {
        let file = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
            .appendingPathComponent("history.bin")
        let history = Data((0 ..< 10).map { UInt8($0) })
        let settings = InMemorySettingsManager()
        let imports = Imports()
        let migration = ProductChatMigration(
            exporter: StagedExport(history: history),
            deviceKey: { Data(repeating: 0x5A, count: 32) },
            settings: settings,
            file: file
        )

        await migration.start(productChat: true) { imports.record($0) }
        let first = try await migration.part(offset: 0, maxLen: 4)
        let rest = try await migration.part(offset: 4, maxLen: 100)

        #expect(first.total == 10)
        #expect(first.data + rest.data == history)
        #expect(imports.all == [Data(repeating: 0x5A, count: 32)])

        await migration.imported()
        #expect(try await migration.part(offset: 0, maxLen: 4).total == 0)

        // A later launch stages nothing again.
        let later = ProductChatMigration(
            exporter: StagedExport(history: history),
            deviceKey: { Data(repeating: 0x5A, count: 32) },
            settings: settings,
            file: file
        )
        await later.start(productChat: true) { imports.record($0) }
        #expect(try await later.part(offset: 0, maxLen: 4).total == 0)
        #expect(imports.all.count == 1)
    }

    @Test func anInstallThatRunsTheNativeChatStagesNothing() async throws {
        let file = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
            .appendingPathComponent("history.bin")
        let imports = Imports()
        let migration = ProductChatMigration(
            exporter: StagedExport(history: Data([1])),
            deviceKey: { Data(repeating: 0x5A, count: 32) },
            settings: InMemorySettingsManager(),
            file: file
        )

        await migration.start(productChat: false) { imports.record($0) }

        #expect(try await migration.part(offset: 0, maxLen: 4).total == 0)
        #expect(imports.all.isEmpty)
    }
}
