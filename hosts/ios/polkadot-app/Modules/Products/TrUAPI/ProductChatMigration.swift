import BigInt
import CoreData
import Foundation
import KeyDerivation
import Keystore_iOS
import Operation_iOS
import SubstrateSdk
import TrUAPIHost

/// The native chat's history, as the chat product imports it (HX3, P16): the SCALE `HistoryExport::V1` of the
/// chat product's `packages/core/src/wire/history.ts`.
protocol NativeChatHistoryExporting: Sendable {
    /// Nil when the native chat has no contact.
    func export() async throws -> Data?
}

final class NativeChatHistoryExporter: NativeChatHistoryExporting, @unchecked Sendable {
    private let contacts: ChatContactRepositoryMaking
    private let messages: ChatMessageRepositoryMaking

    init(
        contacts: ChatContactRepositoryMaking = ChatContactRepositoryFactory(storageFacade: UserDataStorageFacade
            .shared),
        messages: ChatMessageRepositoryMaking = ChatMessageRepositoryFactory()
    ) {
        self.contacts = contacts
        self.messages = messages
    }

    func export() async throws -> Data? {
        let contacts = try await contacts.createRepository(forFilter: nil)
            .fetchAllOperation(with: RepositoryFetchOptions())
            .asyncExecute()
        guard !contacts.isEmpty else { return nil }
        let messages = try await messages.createRepository(forFilter: NSPredicate.messageByChatType(.person))
            .fetchAllOperation(with: RepositoryFetchOptions())
            .asyncExecute()
        return try ProductChatHistoryExport(contacts: contacts, messages: messages).encoded()
    }
}

/// The payments of the native chat, as the host takes them over for the rooms of the chat product (HX3).
protocol NativeChatPaymentsReading: Sendable {
    /// The payments in the rooms of the chat product `productId`.
    func payments(productId: String) async throws -> [AdoptedChatPayment]
}

/// Reads the payment messages of the native chat with the state of their transfers. Only the payments the native
/// transfer monitor followed are taken: every payment of a contact, whose coins it claimed, and the payments of the
/// user that this device made and that did not fail before their coins went, whose status it followed.
final class NativeChatPaymentReader: NativeChatPaymentsReading, @unchecked Sendable {
    private let databaseService: CoreDataServiceProtocol
    private let logger: LoggerProtocol

    init(storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared, logger: LoggerProtocol = Logger.shared) {
        databaseService = storageFacade.databaseService
        self.logger = logger
    }

    func payments(productId: String) async throws -> [AdoptedChatPayment] {
        try await databaseService.performRead { [logger] context in
            let request = NSFetchRequest<CDChatMessage>(entityName: String(describing: CDChatMessage.self))
            request.predicate = NSCompoundPredicate(andPredicateWithSubpredicates: [
                .messageByChatType(.person),
                .messageByContentType(.coinageSend)
            ])
            let mapper = ChatMessageEntityMapper()
            return try context.fetch(request).compactMap { entity in
                do {
                    return try Self.payment(
                        from: mapper.transform(entity: entity),
                        firstAttemptAt: entity.incomingTransferState?.firstAttemptAt,
                        productId: productId
                    )
                } catch {
                    logger.error("A native chat payment was not taken over: \(error)")
                    return nil
                }
            }
        }
    }

    /// The payment of a native payment message in the room of its contact, as the room of the chat product draws it.
    static func payment(
        from message: Chat.LocalMessage,
        firstAttemptAt: Date?,
        productId: String
    ) -> AdoptedChatPayment? {
        guard case let .coinageSend(transfer) = message.content, let account = message.chatId.accountId else {
            return nil
        }

        let incoming = message.status.isIncoming
        if !incoming {
            guard
                message.creationSource == .localDevice,
                message.status != .outgoing(.failed) || transfer.state != nil
            else {
                return nil
            }
        }

        // The product has the message from the history: the host never sends it again.
        let payment = ProductChatPayment(
            messageId: message.messageId,
            productId: productId,
            roomId: "dm-" + account.toHex(),
            incoming: incoming,
            timestamp: message.timestamp,
            totalValue: transfer.totalValue,
            coinKeys: transfer.coinKeys,
            delivery: .delivered,
            state: transfer.state
        )
        return AdoptedChatPayment(payment: payment, firstAttemptAt: incoming ? firstAttemptAt : nil)
    }
}

/// Encodes `HistoryExport::V1`. Messages go as the device sync sends them, so content device sync does not carry
/// (files among them) is not exported.
struct ProductChatHistoryExport {
    let contacts: [Chat.Contact]
    let messages: [Chat.LocalMessage]

    func encoded() throws -> Data {
        let encoder = ScaleEncoder()
        try UInt8(0).encode(scaleEncoder: encoder)
        try encodeContacts(encoder)
        try encodeRequests(encoder)
        try encodeMessages(encoder)
        return encoder.encode()
    }
}

private extension ProductChatHistoryExport {
    static let tokenPlatformAndroid: UInt8 = 0
    static let tokenPlatformIos: UInt8 = 1

    func encodeContacts(_ encoder: ScaleEncoder) throws {
        try BigUInt(contacts.count).encode(scaleEncoder: encoder)
        for contact in contacts {
            encoder.appendRaw(data: contact.accountId)
            try ScaleOption(value: contact.username).encode(scaleEncoder: encoder)
            encoder.appendRaw(data: contact.publicKey)
            try ScaleOption(value: contact.pushToken).encode(scaleEncoder: encoder)
            let platform: UInt8? =
                switch contact.peerPlatform {
                case .android: Self.tokenPlatformAndroid
                case .ios: Self.tokenPlatformIos
                case nil: nil
                }
            try ScaleOption(value: platform).encode(scaleEncoder: encoder)
            try ScaleOption(value: contact.voipPushToken).encode(scaleEncoder: encoder)
            try contact.isBlocked.encode(scaleEncoder: encoder)
            try false.encode(scaleEncoder: encoder)
            try UInt64(milliseconds(contact.addedAt) ?? 0).encode(scaleEncoder: encoder)
            try ScaleOption(value: milliseconds(contact.acceptedAt)).encode(scaleEncoder: encoder)
            try BigUInt(contact.devices.count).encode(scaleEncoder: encoder)
            for device in contact.devices {
                encoder.appendRaw(data: device.statementAccountId)
                encoder.appendRaw(data: device.encryptionPublicKey)
            }
        }
    }

    func encodeRequests(_ encoder: ScaleEncoder) throws {
        let requests = contacts.compactMap(\.chatRequest)
        try BigUInt(requests.count).encode(scaleEncoder: encoder)
        for request in requests {
            try request.requestId.encode(scaleEncoder: encoder)
            encoder.appendRaw(data: request.contactAccountId)
            try request.timestamp.encode(scaleEncoder: encoder)
            try request.isOutgoing.encode(scaleEncoder: encoder)
            let status: UInt8 =
                switch request.status {
                case .outgoing,
                     .incoming(.new): 0
                case .incoming(.accepted): 1
                case .incoming(.declined): 2
                }
            try status.encode(scaleEncoder: encoder)
            try (request.message?.status == .outgoing(.sent)).encode(scaleEncoder: encoder)
            try ScaleOption<String>(value: nil).encode(scaleEncoder: encoder)
        }
    }

    func encodeMessages(_ encoder: ScaleEncoder) throws {
        let wires = messages.compactMap(Chat.DeviceSyncWireMessage.init(from:))
        try BigUInt(wires.count).encode(scaleEncoder: encoder)
        for wire in wires {
            encoder.appendRaw(data: wire.peerId)
            let message = ScaleEncoder()
            try wire.remote.encode(scaleEncoder: message)
            try message.encode().encode(scaleEncoder: encoder)
            try wire.status.encode(scaleEncoder: encoder)
            try wire.order.encode(scaleEncoder: encoder)
        }
    }

    func milliseconds(_ date: Date?) -> UInt64? {
        date.map { UInt64(max(0, $0.timeIntervalSince1970 * 1_000)) }
    }
}

/// Moves this install's native chat to the chat product once, while the native chat's data is still there (HX3):
/// stages its history for the product's import and gives the core the native device key, so contacts keep reaching
/// this device, and takes over the payments of the native chat, so their coins keep being claimed and followed. The
/// product's first `historyExport` waits for it. The staged file sits under Application Support, excluded from backup
/// and readable once the device was first unlocked. The native chat no longer runs: its storage is read here and
/// nowhere else.
actor ProductChatMigration {
    private let exporter: NativeChatHistoryExporting
    private let deviceKey: @Sendable () throws -> Data
    private let nativePayments: NativeChatPaymentsReading
    private let adoptPayments: @Sendable ([AdoptedChatPayment]) async throws -> Int
    private let chatProductId: @Sendable () -> String?
    private let settings: SettingsManagerProtocol
    private let file: URL
    private let logger: LoggerProtocol
    private var staging: Task<Void, Never>?

    init(
        exporter: NativeChatHistoryExporting = NativeChatHistoryExporter(),
        deviceKey: @escaping @Sendable () throws -> Data = {
            try DeviceEncryptionKeyManager.shared.getOrCreatePrivateKey().rawRepresentation
        },
        nativePayments: NativeChatPaymentsReading = NativeChatPaymentReader(),
        adoptPayments: @escaping @Sendable ([AdoptedChatPayment]) async throws -> Int = {
            try await ProductChatPayments.shared.adopt($0)
        },
        chatProductId: @escaping @Sendable () -> String? = {
            DotNsTldProviderFacade.shared.currentTld().map(BuiltInProduct.chat(for:))
        },
        settings: SettingsManagerProtocol = SettingsManager.shared,
        file: URL = ProductChatMigration.defaultFile,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.exporter = exporter
        self.deviceKey = deviceKey
        self.nativePayments = nativePayments
        self.adoptPayments = adoptPayments
        self.chatProductId = chatProductId
        self.settings = settings
        self.file = file
        self.logger = logger
    }

    static var defaultFile: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("ProductChatMigration", isDirectory: true)
            .appendingPathComponent("history.bin")
    }

    /// Stages the history once, and takes over the payments once. A step that fails runs again at the next start.
    func start(importKey: @escaping @Sendable (Data) async throws -> Bool) {
        guard staging == nil else { return }
        staging = Task {
            if settings.bool(for: Self.stagedKey) != true {
                do {
                    try await stage(importKey: importKey)
                    settings.set(value: true, for: Self.stagedKey)
                } catch {
                    logger.error("The native chat history was not staged: \(error)")
                }
            }

            // On its own flag: an install that staged its history before the payments were taken over takes them
            // over now.
            if settings.bool(for: Self.paymentsKey) != true {
                do {
                    if try await adoptNativePayments() {
                        settings.set(value: true, for: Self.paymentsKey)
                    }
                } catch {
                    logger.error("The native chat payments were not taken over: \(error)")
                }
            }
        }
    }

    /// At most `maxLen` bytes from `offset` of the staged history, and its size; a size of 0 when none is staged.
    func part(offset: UInt64, maxLen: UInt32) async throws -> HostHistoryExportChunk {
        await staging?.value
        guard let handle = try? FileHandle(forReadingFrom: file) else {
            return HostHistoryExportChunk(total: 0, data: Data())
        }
        defer { try? handle.close() }
        let total = try handle.seekToEnd()
        try handle.seek(toOffset: min(offset, total))
        let data = try handle.read(upToCount: Int(min(UInt64(maxLen), total - min(offset, total)))) ?? Data()
        return HostHistoryExportChunk(total: total, data: data)
    }

    func imported() {
        try? FileManager.default.removeItem(at: file)
    }
}

private extension ProductChatMigration {
    static let stagedKey = "productChatHistoryStaged"
    static let paymentsKey = "productChatPaymentsAdopted"

    /// Gives false when the chat product is not known yet (no TLD), so it runs again at the next start.
    func adoptNativePayments() async throws -> Bool {
        guard let productId = chatProductId() else { return false }
        let payments = try await nativePayments.payments(productId: productId)
        let adopted = try await adoptPayments(payments)
        logger.info("The chat product took over \(adopted) of \(payments.count) native chat payments")
        return true
    }

    func stage(importKey: @Sendable (Data) async throws -> Bool) async throws {
        guard let history = try await exporter.export() else { return }
        var directory = file.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        try directory.setResourceValues(values)
        try history.write(to: file, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
        if try await !importKey(deviceKey()) {
            logger
                .warning(
                    "The core keeps another chat device key; contacts of the native chat may not reach this device"
                )
        }
    }
}
