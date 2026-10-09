import CoreData
import Foundation
import Operation_iOS
import TrUAPIHost

/// Operation bindings only. Spendable memo material stays in the native Coinage ledger/key derivation.
struct NativeCoinageBinding: Codable, Equatable, Sendable {
    let root: Data
    let genesis: Data
    let instance: UInt32?

    init(_ scope: NativeCoinageScope) {
        root = scope.rootPublicKey
        genesis = scope.genesisHash
        instance = scope.coinageInstanceId
    }

    var key: String { "\(root.toHex()).\(genesis.toHex()).\(instance.map(String.init) ?? "legacy")" }
}

struct NativeCoinageIntent: Codable, Equatable, Sendable {
    let operation: Data
    let product: String
    let request: String
    let peer: Data
    let username: String?
    let cents: UInt64

    init(_ intent: NativeCoinagePaymentIntent) {
        operation = intent.operationId
        product = intent.productId
        request = intent.requestId
        peer = intent.peerIdentity
        username = intent.recipientUsername
        cents = intent.amountCents
    }
}

struct NativeCoinageOutgoing: Codable, Equatable, Sendable {
    enum Approval: String, Codable { case reviewing, approved, rejected }
    let binding: NativeCoinageBinding
    let intent: NativeCoinageIntent
    let timestamp: UInt64
    let centsUnit: String
    var approval: Approval
    var privacyApproved: Bool
    var accepted: Bool
    var delivered: Bool

    var custodyId: String { "truapi.native.payment.\(binding.key).\(intent.operation.toHex())" }
}

struct NativeCoinageIncoming: Codable, Equatable, Sendable {
    let binding: NativeCoinageBinding
    let operation: Data
    let product: String
    let minimum: String
    /// Public source identities remain after native incoming secret storage is wiped at finality.
    let sources: [Data]
}

enum NativeCoinageRecord: Codable, Equatable, Sendable {
    case outgoing(NativeCoinageOutgoing)
    case incoming(NativeCoinageIncoming)

    var binding: NativeCoinageBinding {
        switch self {
        case let .outgoing(value): value.binding
        case let .incoming(value): value.binding
        }
    }

    var operation: Data {
        switch self {
        case let .outgoing(value): value.intent.operation
        case let .incoming(value): value.operation
        }
    }

    var key: String { "truapi.native.intent.\(binding.key).\(operation.toHex())" }
}

protocol NativeCoinageRecordStoring: Sendable {
    func records(binding: NativeCoinageBinding) async throws -> [NativeCoinageRecord]
    func save(_ record: NativeCoinageRecord, authorization: @escaping @Sendable () throws -> Void) async throws
}

/// Uses the wallet's database, not UserDefaults or the Rust purse slot. All writes are awaited saves.
final class TrUAPINativeCoinageStore: NativeCoinageRecordStoring, @unchecked Sendable {
    private let database: CoreDataServiceProtocol

    init(storageFacade: StorageFacadeProtocol) {
        database = storageFacade.databaseService
    }

    func records(binding: NativeCoinageBinding) async throws -> [NativeCoinageRecord] {
        try await database.perform { context in
            let request = NSFetchRequest<NSManagedObject>(entityName: "CDNativeCoinageRecord")
            request.predicate = NSPredicate(format: "identifier BEGINSWITH %@", "truapi.native.intent.\(binding.key).")
            return try context.fetch(request).map { entity in
                guard let payload = entity.value(forKey: "payload") as? Data else {
                    throw HostRejection.Rejected(reason: "Native Coinage journal unavailable")
                }
                let record = try JSONDecoder().decode(NativeCoinageRecord.self, from: payload)
                guard record.binding == binding, entity.value(forKey: "identifier") as? String == record.key else {
                    throw HostRejection.Rejected(reason: "Native Coinage journal unavailable")
                }
                return record
            }
        }
    }

    func save(_ record: NativeCoinageRecord, authorization: @escaping @Sendable () throws -> Void) async throws {
        let payload = try JSONEncoder().encode(record)
        try await database.perform { context in
            do {
                try authorization()
                let request = NSFetchRequest<NSManagedObject>(entityName: "CDNativeCoinageRecord")
                request.predicate = NSPredicate(format: "identifier == %@", record.key)
                let entity = try context.fetch(request).first
                    ?? NSEntityDescription.insertNewObject(forEntityName: "CDNativeCoinageRecord", into: context)
                entity.setValue(record.key, forKey: "identifier")
                entity.setValue(payload, forKey: "payload")
                try context.save()
            } catch {
                context.rollback()
                throw error
            }
        }
    }
}
