import Foundation
import Testing
import TrUAPIHost
import UIKit
@testable import polkadot_app

/// The call of a product room (P10.7, 07-LATER §11.2, scenarios S72 to S74).
@MainActor
struct ChatProductCallTests {
    private typealias Data = ProductRoomTestData

    private let room = RecordingProductRoom()
    private let wireframe = RecordingProductRoomWireframe()
    private let view = RecordingProductRoomView()
    private let calls = ChatRoomCallStore()
    private let events = RecordingRoomEventPublisher()

    // MARK: - Header buttons (S72)

    @Test func declaredCallsShowTheButtonsOfTheirKinds() {
        #expect(ChatRoomPolicy.callButtons(for: features(audio: true, video: true)) == [.audioCall, .videoCall])
        #expect(ChatRoomPolicy.callButtons(for: features(audio: false, video: true)) == [.videoCall])
        #expect(ChatRoomPolicy.callButtons(for: features(audio: true, video: false)) == [.audioCall])
    }

    @Test func aRoomWithoutCallsShowsNoButton() {
        #expect(ChatRoomPolicy.callButtons(for: .hostDefault).isEmpty)
        #expect(ChatRoomPolicy.callButtons(for: nil).isEmpty)
    }

    @Test func aHostWithoutCallButtonsShowsNoButton() {
        let buttons = ChatRoomPolicy.callButtons(for: features(audio: true, video: true)) { _ in false }

        #expect(buttons.isEmpty)
    }

    @Test func thePolicyOfARoomWithCallsHasTheButtonsInItsHeader() {
        var declarations = Data.declarations()
        declarations.features = features(audio: true, video: false)
        let policy = ChatRoomPolicy(footer: nil, declarations: declarations)

        #expect(policy.toolbar == [.audioCall])
        #expect(policy.hasCallButtons)
        #expect(policy.callButtonsE2EDescription == "audio")
    }

    @Test func withCallButtonsEveryProductButtonGoesInTheOverflowMenu() throws {
        let presenter = try makePresenter(calls: features(audio: true, video: true), toolbar: [
            button("tb-1"), button("tb-2")
        ])

        let items = try #require(view.toolbars.last)
        #expect(items.count == 1)
        #expect(items[0].menu?.children.map { ($0 as? UIAction)?.title } == ["tb-1", "tb-2"])
        _ = presenter
    }

    @Test func aCallButtonSendsCallRequested() throws {
        let presenter = try makePresenter()

        presenter.requestCall(video: true)

        #expect(events.events == [published(.callRequested(video: true))])
    }

    // MARK: - Ringing (S73)

    @Test func aRingingCallShowsTheBannerWithTheCaller() async throws {
        let presenter = try makePresenter()

        calls.set(.ringing(callId: "call-1", video: true, caller: nil), for: Data.chatId)
        try await waitUntil { presenter.callBar.viewModel != nil }

        let viewModel = try #require(presenter.callBar.viewModel)
        #expect(viewModel.callId == "call-1")
        #expect(viewModel.video)
        #expect(viewModel.isRinging)
        guard case let .ringing(caller) = viewModel.state else {
            Issue.record("the room must ring")
            return
        }
        #expect(caller.name == Data.productId)
        #expect(view.callBars.last === presenter.callBar)
    }

    @Test func acceptAndDeclineSendCallAnswered() async throws {
        let presenter = try makePresenter()
        calls.set(.ringing(callId: "call-2", video: false, caller: "p1"), for: Data.chatId)
        try await waitUntil { presenter.call != nil }

        presenter.callBar.onAccept()
        presenter.callBar.onDecline()

        #expect(events.events == [
            published(.callAnswered(callId: "call-2", accept: true)),
            published(.callAnswered(callId: "call-2", accept: false))
        ])
    }

    @Test func anAnswerWithoutARingingCallSendsNothing() throws {
        let presenter = try makePresenter()

        presenter.answerCall(accept: true)

        #expect(events.events.isEmpty)
    }

    @Test func theBannerGoesWhenTheProductClearsTheCall() async throws {
        let presenter = try makePresenter()
        calls.set(.ringing(callId: "call-3", video: false, caller: nil), for: Data.chatId)
        try await waitUntil { presenter.callBar.viewModel != nil }

        calls.set(nil, for: Data.chatId)
        try await waitUntil { presenter.callBar.viewModel == nil }

        #expect(presenter.call == nil)
    }

    // MARK: - Call bar (S74)

    @Test func anActiveCallShowsTheBarWithItsStart() async throws {
        let presenter = try makePresenter()

        calls.set(active("call-4"), for: Data.chatId)
        try await waitUntil { presenter.callBar.viewModel != nil }

        #expect(presenter.callBar.viewModel?.state == .active(
            startedAt: Date.fromChatTimestamp(1_800_000_000_000),
            hasMedia: false
        ))
    }

    @Test func openAndEndOfACallWithoutMediaSendTheirEvents() async throws {
        let presenter = try makePresenter()
        calls.set(active("call-5"), for: Data.chatId)
        try await waitUntil { presenter.call != nil }

        presenter.callBar.onOpen()
        presenter.callBar.onEnd()

        #expect(events.events == [
            published(.callOpened(callId: "call-5")),
            published(.callEnded(callId: "call-5"))
        ])
    }

    @Test func openOfACallWithMediaShowsTheCallScreenOfTheHost() async throws {
        let presenter = try makePresenter()
        calls.set(active("call-6", media: Foundation.Data(repeating: 7, count: 32)), for: Data.chatId)
        try await waitUntil { presenter.call != nil }

        presenter.callBar.onOpen()

        #expect(events.events.isEmpty)
    }

    @Test func theTimerCountsMinutesAndHours() {
        let start = Date(timeIntervalSince1970: 1_000)

        #expect(ChatRoomCallViewModel.elapsedText(since: start, now: start.addingTimeInterval(5)) == "0:05")
        #expect(ChatRoomCallViewModel.elapsedText(since: start, now: start.addingTimeInterval(125)) == "2:05")
        #expect(ChatRoomCallViewModel.elapsedText(since: start, now: start.addingTimeInterval(3_725)) == "1:02:05")
        #expect(ChatRoomCallViewModel.elapsedText(since: start, now: start.addingTimeInterval(-3)) == "0:00")
    }

    @Test func theListPreviewOfARingingRoomIsIncomingCall() {
        #expect(ChatRoomCallViewModel.listPreview(of: .ringing(callId: "c", video: false, caller: nil))
            == String(localized: .ChatCall.ringingTitle))
        #expect(ChatRoomCallViewModel.listPreview(of: .ringing(callId: "c", video: true, caller: nil))
            == String(localized: .ChatCall.ringingTitleVideo))
        #expect(ChatRoomCallViewModel.listPreview(of: active("c")) == nil)
        #expect(ChatRoomCallViewModel.listPreview(of: nil) == nil)
    }
}

private extension ChatProductCallTests {
    func features(audio: Bool, video: Bool) -> Chat.RoomFeatures {
        Chat.RoomFeatures(
            preimageAttachments: false,
            search: false,
            calls: Chat.RoomCalls(audio: audio, video: video),
            threadOf: nil,
            readAloud: false
        )
    }

    func active(_ callId: String, media: Foundation.Data? = nil) -> Chat.RoomCall {
        .active(callId: callId, video: false, startedAt: 1_800_000_000_000, mediaSession: media)
    }

    func button(_ id: String) -> Chat.RoomToolbarAction {
        Chat.RoomToolbarAction(
            actionId: id,
            label: id,
            icon: .inline(mimeType: "image/png", data: Foundation.Data()),
            handler: .callback
        )
    }

    func published(_ event: Chat.RoomEvent) -> RecordingRoomEventPublisher.Published {
        RecordingRoomEventPublisher.Published(event: event, productId: Data.productId, roomId: "lobby")
    }

    func makePresenter(
        calls declared: Chat.RoomFeatures? = nil,
        toolbar: [Chat.RoomToolbarAction]? = nil
    ) throws -> ChatProductRoomPresenter {
        let presenter = try #require(ChatProductRoomPresenter(
            chatId: Data.chatId,
            room: room,
            wireframe: wireframe,
            resolver: NoImageResolver(),
            calls: calls,
            roomEvents: events,
            logger: StubLogger()
        ))
        presenter.view = view
        var declarations = Data.declarations(toolbar: toolbar)
        declarations.features = declared ?? .hostDefault
        presenter.update(policy: ChatRoomPolicy(footer: nil, declarations: declarations), messages: [])
        return presenter
    }

    func waitUntil(_ condition: @escaping @MainActor () -> Bool) async throws {
        let deadline = ContinuousClock.now + .seconds(10)
        while !condition() {
            guard ContinuousClock.now < deadline else {
                Issue.record("the condition did not become true in time")
                return
            }
            try await Task.sleep(for: .milliseconds(10))
        }
    }
}
