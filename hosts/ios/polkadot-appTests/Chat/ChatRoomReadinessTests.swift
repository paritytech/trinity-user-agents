import Foundation
import Operation_iOS
import PolkadotUI
import Testing
@testable import polkadot_app

/// The room screen opens and closes the room of its product.
@MainActor
struct ChatRoomReadinessTests {
    private let engine = RecordingChatEngine()
    private let output = RecordingChatInteractorOutput()

    @Test func openAndCloseReachTheEngineInOrder() async {
        let interactor = makeInteractor()

        interactor.openRoom()
        interactor.closeRoom()
        interactor.openRoom()

        #expect(await calls(3) == [.openRoom, .closeRoom, .openRoom])
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

    /// The native chat shows no row while it loads messages, and none for a page that failed: the chat asks a failed
    /// page again when the user scrolls near its end.
    @Test func aFailedPageIsAskedAgainWithoutARow() {
        let paging = ChatViewLayout.ViewModel.Paging(paging: ChatPagingState(older: .failed, newer: .loading))

        #expect(paging.older == .more)
        #expect(paging.newer == .loading)
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
