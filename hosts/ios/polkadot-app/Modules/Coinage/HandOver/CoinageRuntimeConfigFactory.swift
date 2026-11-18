import Coinage
import CoreData
import Foundation
import NovaCrypto
import Operation_iOS
import SubstrateSdk
import SubstrateSdkExt
import TrUAPIHost

/// The coinage config the shared runtime starts with.
protocol CoinageRuntimeConfigProviding: Sendable {
    /// Synchronous because the runtime is built on demand by synchronous callers. `nil` keeps coinage
    /// off for this runtime: the hand-over could not be read, and the core records one only on its
    /// first start, so it must not start without it.
    func coinageConfig() -> CoinageConfig?
}

/// Starts the core's coinage layer with the installation registry from the remote config and with the
/// wallet the app's own coinage engine kept, which the core takes over the first time it starts and
/// ignores on every later start.
final class CoinageRuntimeConfigFactory: CoinageRuntimeConfigProviding, @unchecked Sendable {
    private let databaseService: CoreDataServiceProtocol
    private let registry: AccountDataStoreConfigProviding
    private let keyFactory: SNKeyFactoryProtocol
    private let logger: LoggerProtocol

    init(
        storageFacade: StorageFacadeProtocol,
        registry: AccountDataStoreConfigProviding,
        keyFactory: SNKeyFactoryProtocol = SNKeyFactory(),
        logger: LoggerProtocol
    ) {
        databaseService = storageFacade.databaseService
        self.registry = registry
        self.keyFactory = keyFactory
        self.logger = logger
    }

    func coinageConfig() -> CoinageConfig? {
        let handOver: Handover?
        do {
            handOver = try databaseService.performReadAndWait(buildHandOver)
        } catch {
            logger.error("Coinage hand-over could not be read, coinage stays off: \(error)")
            return nil
        }

        logger.info(
            "Core coinage config: registry=\(registry.contractAddress != nil) "
                + "handOverInstallations=\(handOver?.installations.count ?? 0)"
        )

        return CoinageConfig(
            handOver: handOver,
            registryContract: registry.contractAddress?.toHex(includePrefix: true)
        )
    }
}

private extension CoinageRuntimeConfigFactory {
    /// `nil` when the app never ran its own engine on this seed.
    func buildHandOver(context: NSManagedObjectContext) throws -> Handover? {
        let installations = try installationRows(in: context).map { try appInstallation($0, in: context) }
        guard !installations.isEmpty else { return nil }

        let mapper = ChatMessageEntityMapper()
        let outgoing = try context.fetch(Self.messages(.outgoingLocalDeviceCoinageSendMessages()))
            .compactMap { (try? mapper.transform(entity: $0)).flatMap(PaymentMessage.init) }
        let incoming = try context.fetch(Self.messages(.incomingCoinageSendMessages()))
            .compactMap { (try? mapper.transform(entity: $0)).flatMap(PaymentMessage.init) }

        let merchantMapper = W3sPaymentRecordMapper()
        let merchantPayments = try context.fetch(NSFetchRequest<CDPaymentRecord>(entityName: "CDPaymentRecord"))
            .compactMap { try? merchantMapper.transform(entity: $0) }
            .filter(\.isOutstanding)

        // A payment whose secrets do not decode controls nothing anyone can claim, so it is left out
        // rather than holding back the rest of the wallet.
        return Handover(
            installations: installations,
            outstanding: outgoing.compactMap { try? outstandingPayment($0) }
                + merchantPayments.compactMap { try? outstandingPayment($0) },
            incoming: incoming.compactMap { try? incomingPayment($0) }
        )
    }

    /// The current installation and every previous one the app's recovery found, with how far the
    /// recovery scan of each got.
    func installationRows(in context: NSManagedObjectContext) throws -> [InstallationRow] {
        let current = try context.fetch(NSFetchRequest<CDCurrentInstallation>(entityName: "CDCurrentInstallation"))
            .compactMap(\.identifier)
            .map { InstallationRow(hex: $0, coinScanNextIndex: 0, voucherScanNextIndex: 0) }
        let previous = try context.fetch(NSFetchRequest<CDCoinageInstallation>(entityName: "CDCoinageInstallation"))
            .compactMap { row in
                row.identifier.map {
                    InstallationRow(
                        hex: $0,
                        coinScanNextIndex: row.coinScanNextIndex,
                        voucherScanNextIndex: row.voucherScanNextIndex
                    )
                }
            }

        var seen = Set<String>()
        return (current + previous).filter { seen.insert($0.hex).inserted }
    }

    /// The marks cover every index the engine allocated, and every index its own recovery scan passed.
    func appInstallation(_ row: InstallationRow, in context: NSManagedObjectContext) throws -> AppInstallation {
        let nextCoin = try nextIndex(entityName: "CDCoin", installation: row.hex, in: context)
        let nextEntry = try nextIndex(entityName: "CDVoucher", installation: row.hex, in: context)

        return try AppInstallation(
            id: row.hex.fromHex(),
            nextCoin: UInt32(clamping: max(nextCoin, row.coinScanNextIndex)),
            nextEntry: UInt32(clamping: max(nextEntry, row.voucherScanNextIndex))
        )
    }

    /// One past the highest index allocated under `installation`, or 0 when none was.
    func nextIndex(entityName: String, installation: String, in context: NSManagedObjectContext) throws -> Int64 {
        let request = NSFetchRequest<NSManagedObject>(entityName: entityName)
        request.predicate = NSPredicate(format: "installationId == %@", installation)
        request.sortDescriptors = [NSSortDescriptor(key: "derivationIndex", ascending: false)]
        request.fetchLimit = 1

        guard let highest = try context.fetch(request).first?.value(forKey: "derivationIndex") as? Int64 else {
            return 0
        }
        return highest + 1
    }

    func outstandingPayment(_ payment: PaymentMessage) throws -> OutstandingPayment {
        try OutstandingPayment(
            id: CoinageOperationId.chatSend(messageId: payment.messageId).value,
            accounts: payment.coinKeys.map { try keyFactory.createPublicKey(fromSecret: $0).rawData() }
        )
    }

    func outstandingPayment(_ record: W3sPaymentRecord) throws -> OutstandingPayment {
        try OutstandingPayment(
            id: W3sPaymentRecord.exportId(paymentId: record.paymentId).value,
            accounts: record.memo.entries.map { try keyFactory.createPublicKey(fromSecret: $0).rawData() }
        )
    }

    /// Throws for a secret the core could not take, since one such secret would refuse the whole config.
    func incomingPayment(_ payment: PaymentMessage) throws -> IncomingPayment {
        for secret in payment.coinKeys {
            _ = try keyFactory.createPublicKey(fromSecret: secret)
        }

        let claimBy = Date.fromChatTimestamp(payment.timestamp).addingTimeInterval(CoinageConstants.claimRetryWindow)

        return try IncomingPayment(
            id: CoinageOperationId.chatClaim(messageId: payment.messageId).value,
            claimBy: UInt64(max(claimBy.timeIntervalSince1970, 0)),
            coins: payment.coinKeys
        )
    }

    static func messages(_ predicate: NSPredicate) -> NSFetchRequest<CDChatMessage> {
        let request = NSFetchRequest<CDChatMessage>(entityName: "CDChatMessage")
        request.predicate = predicate
        return request
    }
}

private extension W3sPaymentRecord {
    /// A merchant payment whose keys may have left and whose coins nobody was seen to take.
    var isOutstanding: Bool {
        switch status {
        case .pending,
             .submitted,
             .sent,
             .failed: true
        case .claimed,
             .revoked: false
        }
    }
}

/// A chat message that carries a payment.
private struct PaymentMessage {
    let messageId: Chat.MessageId
    let timestamp: Chat.Timestamp
    let coinKeys: [Data]

    init?(_ message: Chat.LocalMessage) {
        guard case let .coinageSend(content) = message.content else { return nil }

        messageId = message.messageId
        timestamp = message.timestamp
        coinKeys = content.coinKeys
    }
}

private struct InstallationRow {
    let hex: String
    let coinScanNextIndex: Int64
    let voucherScanNextIndex: Int64
}
