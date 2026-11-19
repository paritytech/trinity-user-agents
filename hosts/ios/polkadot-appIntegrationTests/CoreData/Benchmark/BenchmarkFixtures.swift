import Coinage
import DurableTransactions
import Foundation

@testable import polkadot_app

/// Deterministic value models built by index, so seeded ids are reproducible across runs and variants.
enum BenchmarkFixtures {
    static func accountId(_ index: Int) -> Data {
        var bytes = Data(repeating: 0xAB, count: 32)
        withUnsafeBytes(of: UInt32(index).bigEndian) { bytes.replaceSubrange(0 ..< 4, with: $0) }
        return bytes
    }

    static func key(_ index: Int) -> PublicKey {
        var bytes = Data(repeating: 0xCD, count: 32)
        withUnsafeBytes(of: UInt32(index).bigEndian) { bytes.replaceSubrange(28 ..< 32, with: $0) }
        return bytes
    }

    static func contact(index: Int) -> Chat.Contact {
        Chat.Contact(
            accountId: accountId(index),
            username: "bench\(index)",
            publicKey: key(index),
            pin: nil,
            pushId: nil,
            pushToken: nil,
            voipPushToken: nil,
            peerPlatform: nil,
            lastOwnToken: nil,
            voipLastOwnToken: nil,
            chatRequest: nil,
            ownKeyId: .init(signKeyId: "sign-\(index)", encryptionKeyId: "encrypt-\(index)"),
            imageData: nil,
            isBlocked: false,
            devices: [],
            pendingDevicesFanOut: false
        )
    }

    static func chat(for contact: Chat.Contact) -> Chat.LocalModel {
        .newChatWithContact(contact)
    }

    static func chatId(for contact: Chat.Contact) -> Chat.Id {
        .person(contact.accountId)
    }

    static func messageId(chatIndex: Int, item: Int) -> String {
        "seed-\(chatIndex)-\(item)"
    }

    static func message(id: String, index: Int, chatId: Chat.Id, text: String) -> Chat.LocalMessage {
        let origin: Chat.LocalMessage.Origin =
            if let accountId = chatId.accountId {
                .contact(accountId)
            } else {
                .user
            }

        return Chat.LocalMessage(
            messageId: id,
            chatId: chatId,
            origin: origin,
            creationSource: .localDevice,
            status: .incoming(.seen),
            timestamp: UInt64(index),
            content: .text(text),
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )
    }

    static func coin(index: Int) -> Coin {
        Coin(
            exponent: 0,
            derivationIndex: CoinageKeyIndex(installation: .test, item: UInt64(index)),
            age: nil,
            publicKey: key(index)
        )
    }

    static func voucher(index: Int) -> Voucher {
        let now = Date()
        return Voucher(
            exponent: 0,
            derivationIndex: CoinageKeyIndex(installation: .test, item: UInt64(index)),
            allocatedAt: now,
            readyAt: now,
            publicKey: key(index)
        )
    }

    /// Received-input form: no local coin row is required, so the benchmark measures the ledger write
    /// path without coupling to the own-asset mapper.
    static func registration(index: Int, groupId: CoinageTxGroupId) -> CoinageTxRegistration {
        var hash = Data(repeating: 0x5E, count: 32)
        withUnsafeBytes(of: UInt32(index).bigEndian) { hash.replaceSubrange(0 ..< 4, with: $0) }

        return CoinageTxRegistration(
            txHash: hash,
            checkpoint: BlockRef(number: 100, hash: Data([100])),
            mortalityBlocks: 64,
            groupId: groupId,
            inputs: [.coin(.received(hash))],
            outputs: []
        )
    }
}
