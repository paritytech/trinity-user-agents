import Foundation
import SwiftUI
import UIKit

/// The call of a product room (P10.7, 07-LATER §11.2, scenarios S72 to S74): the call buttons of the header, the
/// ringing banner, the call bar, the call screen shell, and the call events that the user sends to the product.
extension ChatProductRoomPresenter {
    /// True when the header shows a call button: the room declares `calls` and this build serves `call_buttons`.
    var hasCallButtons: Bool {
        policy?.hasCallButtons == true
    }

    /// Follows the call of the room. The product sets it with `set_room_call`.
    func followCall(_ store: ChatRoomCallStoring) -> Task<Void, Never> {
        ProductRoomScreens.register(self)
        return Task { [weak self, chatId] in
            for await call in store.callUpdates(for: chatId) {
                guard let self else { return }
                show(call)
            }
        }
    }

    /// A call button of the header: the product starts the call its own way.
    func requestCall(video: Bool) {
        mark("call-request", "video=\(video)")
        publishRoomEvent(.callRequested(video: video))
    }

    /// Accept or Decline of the ringing banner. The product clears the call or sets it `Active`.
    func answerCall(accept: Bool) {
        guard case let .ringing(callId, _, _) = call else { return }
        publishRoomEvent(.callAnswered(callId: callId, accept: accept))
    }

    /// A tap on the call bar. Without a session of the Media service the product opens its own call page, so the
    /// host sends `CallOpened`. With a session the host opens its call screen.
    func openCall() {
        guard case let .active(callId, _, startedAt, mediaSession) = call else { return }
        guard mediaSession != nil else {
            publishRoomEvent(.callOpened(callId: callId))
            return
        }
        guard let controller = view?.controller else { return }

        ChatRoomCallScreen.present(
            from: controller,
            title: appearanceFactory.productName,
            participants: [callParticipant(nil)],
            startedAt: Date.fromChatTimestamp(startedAt)
        ) { [weak self] in
            self?.publishRoomEvent(.callEnded(callId: callId))
        }
    }

    /// End of the call bar.
    func endCall() {
        guard case let .active(callId, _, _, _) = call else { return }
        publishRoomEvent(.callEnded(callId: callId))
    }

    /// Gives the call to the room screen when what the screen shows changes, for example when the product resolves
    /// the name of the caller.
    func provideCall() {
        let viewModel = callViewModel()
        guard viewModel != callBar.viewModel else { return }
        withAnimation(.easeInOut(duration: 0.2)) {
            callBar.viewModel = viewModel
        }
        view?.didReceive(callBar: callBar)
    }

    /// Sends the presses of the ringing banner and of the call bar to this room.
    func bindCallBar() {
        callBar.onAccept = { [weak self] in self?.answerCall(accept: true) }
        callBar.onDecline = { [weak self] in self?.answerCall(accept: false) }
        callBar.onOpen = { [weak self] in self?.openCall() }
        callBar.onEnd = { [weak self] in self?.endCall() }
    }

    /// The call of the room as the room screen shows it.
    func callViewModel() -> ChatRoomCallViewModel? {
        switch call {
        case nil:
            nil
        case let .ringing(callId, video, caller):
            ChatRoomCallViewModel(callId: callId, video: video, state: .ringing(caller: callParticipant(caller)))
        case let .active(callId, video, startedAt, mediaSession):
            ChatRoomCallViewModel(
                callId: callId,
                video: video,
                state: .active(
                    startedAt: Date.fromChatTimestamp(startedAt),
                    hasMedia: mediaSession != nil
                )
            )
        }
    }
}

private extension ChatProductRoomPresenter {
    func show(_ call: Chat.RoomCall?) {
        self.call = call
        if case let .ringing(_, _, caller?) = call, asksParticipants {
            participants.resolve([caller])
        }
        provideCall()
        mark("call", call?.e2eDescription ?? "none")
    }

    /// The participant that calls, with the name that the product resolved. Nil is the product.
    func callParticipant(_ participantId: String?) -> ChatRoomCallViewModel.Caller {
        let appearance = participantId.map { participantId in
            participants.participant(participantId).map { appearanceFactory.appearance(of: $0) }
                ?? appearanceFactory.unresolved(participantId)
        } ?? appearanceFactory.appearance(of: ChatRoomParticipant(
            participantId: Chat.ProductParticipant.product,
            kind: .roomProduct
        ))

        return ChatRoomCallViewModel.Caller(
            name: appearance.name,
            initials: appearance.initials,
            colorSeed: participantId ?? Chat.ProductParticipant.product
        )
    }
}

extension ChatProductRoomViewProtocol {
    func didReceive(callBar _: ChatRoomCallBarModel) {}
}
