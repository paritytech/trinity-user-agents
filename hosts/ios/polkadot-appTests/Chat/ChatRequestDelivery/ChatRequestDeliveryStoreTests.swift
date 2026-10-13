import CoreData
import Foundation
import StructuredConcurrency
import Testing
@testable import polkadot_app

struct ChatRequestDeliveryStoreTests {
    private let requestId = "request"
    private let facade = UserDataStorageTestFacade()

    @Test func awaitsDeliveryWhileOutgoingUnsentAndLinkedToContact() async throws {
        try await insertRequest()

        #expect(try await makeStore().isAwaitingDelivery(requestId: requestId))
    }

    @Test func doesNotAwaitDeliveryOnceContactPointsElsewhere() async throws {
        try await insertRequest(linkedToContact: false)

        #expect(try await !makeStore().isAwaitingDelivery(requestId: requestId))
    }

    @Test func doesNotAwaitDeliveryOfUnknownRequest() async throws {
        #expect(try await !makeStore().isAwaitingDelivery(requestId: requestId))
    }

    @Test func markDeliveredSendsWelcomeMessageAndRecordsPeriod() async throws {
        try await insertRequest()
        let store = makeStore()

        try await store.markDelivered(requestId: requestId, anonymousPeriod: 20_000)

        let (period, status) = try await readBack()
        #expect(period == 20_000)
        #expect(status == Chat.LocalMessage.Status.outgoing(.sent).rawValue)
        #expect(try await !store.isAwaitingDelivery(requestId: requestId))
    }

    @Test func markDeliveredKeepsPeriodAboveInt32Range() async throws {
        try await insertRequest()

        try await makeStore().markDelivered(requestId: requestId, anonymousPeriod: UInt32.max)

        #expect(try await readBack().period == UInt32.max)
    }

    @Test func markFailedEndsDeliveryWithTerminalStatus() async throws {
        try await insertRequest()
        let store = makeStore()

        try await store.markFailed(requestId: requestId)

        let (period, status) = try await readBack()
        #expect(status == Chat.LocalMessage.Status.outgoing(.failed).rawValue)
        #expect(period == nil)
        #expect(try await !store.isAwaitingDelivery(requestId: requestId))
    }

    private func makeStore() -> ChatRequestDeliveryStore {
        ChatRequestDeliveryStore(storageFacade: facade)
    }

    private func insertRequest(linkedToContact: Bool = true) async throws {
        try await facade.databaseService.performWrite { [requestId] context in
            let contact = CDChatContact(context: context)
            contact.username = "peer"
            contact.publicKey = Data(repeating: 1, count: 32)

            let message = CDChatMessage(context: context)
            message.messageId = requestId
            message.status = Chat.LocalMessage.Status.outgoing(.new).rawValue

            let request = CDChatRequest(context: context)
            request.identifier = requestId
            request.status = Chat.RequestStatus.outgoing.rawValue
            request.message = message
            request.contact = linkedToContact ? contact : nil
        }
    }

    private func readBack() async throws -> (period: UInt32?, status: Int16?) {
        try await facade.databaseService.performRead { [requestId] context in
            let fetchRequest = CDChatRequest.fetchRequest()
            fetchRequest.predicate = .chatRequestById(requestId)
            let request = try context.fetch(fetchRequest).first

            let period = request?.anonymousDeliveryPeriod.map { UInt32(bitPattern: $0.int32Value) }

            return (period, request?.message?.status)
        }
    }
}
