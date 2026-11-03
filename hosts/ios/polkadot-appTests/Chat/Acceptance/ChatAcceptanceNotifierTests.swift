import Foundation
import Operation_iOS
import os
import Testing
@testable import polkadot_app

struct ChatAcceptanceNotifierTests {
    private let fixture = ChatAcceptanceFixture()
    private let apns = RecordingAPNSClient()
    private let facade = UserDataStorageTestFacade()

    @Test func pushesAcceptanceOnce() async {
        let notifier = makeNotifier()
        let input = fixture.makeInput()

        await notifier.didPublishFirstTime(input)
        await notifier.didPublishFirstTime(input)

        #expect(apns.pushedMessageIds == ["acceptance"])
    }

    @Test func failedPushIsRetriedNextTime() async {
        let notifier = makeNotifier()
        let input = fixture.makeInput()
        apns.failNext = true

        await notifier.didPublishFirstTime(input)
        await notifier.didPublishFirstTime(input)

        #expect(apns.pushedMessageIds == ["acceptance", "acceptance"])
    }
}

private extension ChatAcceptanceNotifierTests {
    func makeNotifier() -> ChatAcceptanceNotifier {
        ChatAcceptanceNotifier(
            apnsClientService: apns,
            notifiedMessageIdRepository: NotifiedMessageIdRepositoryFactory(storageFacade: facade).createRepository(),
            logger: MockLogger()
        )
    }
}

private struct PushTestError: Error {}

private final class RecordingAPNSClient: APNSClientServicing, @unchecked Sendable {
    private let state = OSAllocatedUnfairLock<(pushed: [String], failNext: Bool)>(initialState: ([], false))

    var pushedMessageIds: [String] { state.withLock { $0.pushed } }

    var failNext: Bool {
        get { state.withLock { $0.failNext } }
        set { state.withLock { $0.failNext = newValue } }
    }

    func notifyAboutNewMessageWrapper(
        _ message: Chat.RemoteMessage,
        contact _: Chat.Contact
    ) -> CompoundOperationWrapper<NotifyResponse> {
        let shouldFail = state.withLock { state in
            state.pushed.append(message.messageId)
            defer { state.failNext = false }
            return state.failNext
        }

        guard !shouldFail else { return .createWithError(PushTestError()) }

        return .createWithResult(NotifyResponse(success: true, platform: "ios", sent: 1, failed: 0, messageId: nil))
    }
}
