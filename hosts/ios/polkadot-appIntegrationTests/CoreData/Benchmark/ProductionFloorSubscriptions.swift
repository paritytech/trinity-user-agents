import AsyncExtensions
import Coinage
import CoreData
import Foundation
import Operation_iOS
import StructuredConcurrency
import SubstrateSdk

@testable import polkadot_app

/// The live-subscription floor of the app with a chat open: 7 message, 3 chat, 4 contact, 3 coin and
/// 3 voucher snapshot subscriptions, through the production `subscribeSnapshot` path. Indices are
/// stable regardless of which groups are enabled; index 0 is always the unfiltered message stream.
final class ProductionFloorSubscriptions {
    enum Group: CaseIterable {
        case messages
        case chats
        case contacts
        case coins
        case vouchers
    }

    static let unfilteredMessages = 0
    static let unfilteredCoins = 14
    static let unfilteredVouchers = 17

    let tracker = DeliveryTracker()
    let chatMapperCalls = MapperCallCounter()
    let assetMapperCalls = MapperCallCounter()

    private let facade: BenchmarkStorageFacade
    private let chatIds: [Chat.Id]
    private let groups: Set<Group>
    private var tasks: [Task<Void, Never>] = []

    init(facade: BenchmarkStorageFacade, chatIds: [Chat.Id], groups: Set<Group> = Set(Group.allCases)) {
        self.facade = facade
        self.chatIds = chatIds
        self.groups = groups
    }

    func start() {
        if groups.contains(.messages) { startMessages() }
        if groups.contains(.chats) { startChats() }
        if groups.contains(.contacts) { startContacts() }
        if groups.contains(.coins) { startCoins() }
        if groups.contains(.vouchers) { startVouchers() }
    }

    func stop() {
        tasks.forEach { $0.cancel() }
        tasks.removeAll()
    }

    var counters: [String: Int] {
        [
            "chatMapperCalls": chatMapperCalls.value,
            "assetMapperCalls": assetMapperCalls.value,
            "deliveries": tracker.totalDeliveries()
        ]
    }
}

private extension ProductionFloorSubscriptions {
    func startMessages() {
        track(subscribe(ChatMessageEntityMapper(), counter: chatMapperCalls, filter: nil), index: 0)

        for offset in 0 ..< 6 {
            let chatId = chatIds[offset % chatIds.count]
            let stream = subscribe(
                ChatMessageEntityMapper(),
                counter: chatMapperCalls,
                filter: .localMessages(from: chatId)
            )
            track(stream, index: 1 + offset)
        }
    }

    func startChats() {
        let byId: NSPredicate = .chat(for: chatIds[0].rawRepresentation)
        track(subscribe(ChatModelMapper(), counter: chatMapperCalls, filter: nil), index: 7)
        track(subscribe(ChatModelMapper(), counter: chatMapperCalls, filter: byId), index: 8)
        track(subscribe(ChatModelMapper(), counter: chatMapperCalls, filter: nil), index: 9)
    }

    func startContacts() {
        let contactId = BenchmarkFixtures.contact(index: 0).identifier
        let byId = NSPredicate(format: "%K == %@", #keyPath(CDChatContact.identifier), contactId)
        track(subscribe(ChatContactMapper(), counter: chatMapperCalls, filter: nil), index: 10)
        track(subscribe(ChatContactMapper(), counter: chatMapperCalls, filter: nil), index: 11)
        track(subscribe(ChatContactMapper(), counter: chatMapperCalls, filter: byId), index: 12)
        track(subscribe(ChatContactMapper(), counter: chatMapperCalls, filter: byId), index: 13)
    }

    func startCoins() {
        let keys = (0 ..< 10).map { BenchmarkFixtures.key($0).toHex() }
        let byKeys = NSPredicate(format: "%K IN %@", #keyPath(CDCoin.publicKey), keys)
        track(subscribe(TrackedCoinMapper(), counter: assetMapperCalls, filter: nil), index: 14)
        track(subscribe(TrackedCoinMapper(), counter: assetMapperCalls, filter: byKeys), index: 15)
        track(subscribe(TrackedCoinMapper(), counter: assetMapperCalls, filter: nil), index: 16)
    }

    func startVouchers() {
        for index in 17 ..< 20 {
            track(subscribe(TrackedVoucherMapper(), counter: assetMapperCalls, filter: nil), index: index)
        }
    }

    func subscribe<M: CoreDataMapperProtocol>(
        _ mapper: M,
        counter: MapperCallCounter,
        filter: NSPredicate?
    ) -> AnyAsyncSequence<[M.DataProviderModel]>
        where M.DataProviderModel: Identifiable, M.CoreDataEntity: NSManagedObject {
        facade.databaseService.subscribeSnapshot(
            mapper: AnyCoreDataMapper(CountingMapper(mapper, counter: counter)),
            filter: filter
        )
    }

    func track(_ stream: AnyAsyncSequence<[some Identifiable]>, index: Int) {
        let tracker = tracker
        let task = Task {
            do {
                for try await models in stream {
                    tracker.recordDelivery(subscription: index, size: models.count)
                }
            } catch {
                print("[bench] subscription \(index) ended with \(error)")
            }
        }
        tasks.append(task)
    }
}
