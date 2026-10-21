import Foundation
import Operation_iOS
import PolkadotUI
import Testing
@testable import polkadot_app

/// The room screen opens and closes the room of its product, and shows when the product does not answer (P5.7).
@MainActor
struct ChatRoomReadinessTests {
    private let engine = RecordingChatEngine()
    private let output = RecordingChatInteractorOutput()

    @Test func openCloseAndRetryReachTheEngineInOrder() async {
        let interactor = makeInteractor()

        interactor.openRoom()
        interactor.closeRoom()
        interactor.openRoom()
        interactor.retryRoom()

        #expect(await calls(4) == [.openRoom, .closeRoom, .openRoom, .retryRoom])
    }

    @Test func theReadinessOfTheWindowReachesThePresenter() async throws {
        engine.snapshotUpdates = [MessageListSnapshot(messages: [], readiness: .unavailable)]
        let interactor = makeInteractor()

        interactor.setup()
        for await event in output.events where event == .listModel {
            break
        }

        #expect(try #require(output.listModel).readiness == .unavailable)
    }

    @Test func aReadyRoomShowsNoStatusLine() {
        #expect(ChatViewLayout.ViewModel.RoomStatus(readiness: .ready) == nil)
    }

    @Test func aLoadingRoomShowsASpinnerLine() {
        guard case .loading = ChatViewLayout.ViewModel.RoomStatus(readiness: .loading) else {
            Issue.record("a loading room shows the loading line")
            return
        }
    }

    @Test func anUnavailableRoomOffersARetry() {
        guard case let .unavailable(text, retryTitle) = ChatViewLayout.ViewModel.RoomStatus(readiness: .unavailable)
        else {
            Issue.record("an unavailable room shows the retry line")
            return
        }

        #expect(!text.isEmpty)
        #expect(!retryTitle.isEmpty)
    }

    @Test func aFailedPageShowsATextRow() {
        let paging = ChatViewLayout.ViewModel.Paging(paging: ChatPagingState(older: .failed, newer: .loading))

        #expect(paging.newer == .loading)
        guard case let .failed(text) = paging.older else {
            Issue.record("a failed page shows a text row")
            return
        }
        #expect(!text.isEmpty)
    }
}

private extension ChatRoomReadinessTests {
    func makeInteractor() -> ChatInteractor {
        let reactions = ChatReactionRepository(
            repository: AnyDataProviderRepository(InMemoryDataProviderRepository<Chat.MessageReaction>())
        )
        let interactor = ChatInteractor(
            chatId: .chatExtension("product.dot", roomId: "room"),
            engine: engine,
            reactionRepository: reactions,
            usernameStorage: StubUsernameStorage(),
            foregroundVisibilityReporter: nil,
            notificationsCleaner: StubPushNotificationsCleaner()
        )
        interactor.presenter = output
        return interactor
    }

    func calls(_ count: Int) async -> [RecordingChatEngine.Call] {
        var calls: [RecordingChatEngine.Call] = []
        for await call in engine.calls {
            calls.append(call)
            if calls.count == count {
                break
            }
        }
        return calls
    }
}
