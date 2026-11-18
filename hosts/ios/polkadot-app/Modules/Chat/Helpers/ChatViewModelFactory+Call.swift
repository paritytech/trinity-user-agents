import Foundation
import PolkadotUI

// MARK: - Call bubble rendering

extension ChatViewModelFactory {
    func callConfigurations(
        for payload: Chat.LocalMessage.Content.CallSignalingPayload,
        message: Chat.LocalMessage,
        status: Chat.LocalMessage.Status,
        isSeenByPeer: Bool,
        deliveryDate: Date?,
        actions: ChatViewModelActions
    ) -> [IdentifiableAnyContentConfiguration<ChatViewLayout.ItemIdentifierType>] {
        switch payload {
        case let .offer(offerContent):
            [
                callMessageConfiguration(
                    message: message,
                    offer: offerContent,
                    status: status,
                    isSeenByPeer: isSeenByPeer,
                    deliveryDate: deliveryDate,
                    actions: actions
                )
            ]
        case .answer,
             .candidates,
             .closed:
            []
        }
    }
}

private extension ChatViewModelFactory {
    // swiftlint:disable:next cyclomatic_complexity
    func callMessageConfiguration(
        message: Chat.LocalMessage,
        offer: Chat.RemoteMessageContentV1.MessageContent.DataChannelOfferContent,
        status: Chat.LocalMessage.Status,
        isSeenByPeer: Bool,
        deliveryDate: Date?,
        actions: ChatViewModelActions
    ) -> IdentifiableAnyContentConfiguration<ChatViewLayout.ItemIdentifierType> {
        let callState = message.resolveCallState() ?? .calling
        let callType: ChatCallMessageConfiguration.CallType =
            switch offer.purpose {
            case .audio: .audio
            case .video: .video
            }
        let chatCallType: ChatCallType =
            switch offer.purpose {
            case .audio: .audio
            case .video: .video
            }
        let uiState: ChatCallMessageConfiguration.State =
            switch callState {
            case .calling:
                .calling
            case .active:
                .active
            case let .finished(duration):
                .finished(duration: formatCallDuration(duration))
            case .cancelled:
                .cancelled
            case .missed:
                .missed
            }
        let onTap: (() -> Void)? =
            switch callState {
            case .missed,
                 .cancelled:
                { actions.startCall(chatCallType) }
            case .calling,
                 .active,
                 .finished:
                nil
            }

        let configuration: ChatMessageContainerConfiguration =
            switch status {
            case .incoming:
                ChatCallMessageConfiguration.inbox(
                    callType: callType,
                    state: uiState,
                    statusConfiguration: .inbox(date: deliveryDate, formatter: timeFormatter),
                    onTap: onTap
                )
            case let .outgoing(outgoingStatus):
                ChatCallMessageConfiguration.outbox(
                    callType: callType,
                    state: uiState,
                    statusConfiguration: .outbox(
                        date: deliveryDate,
                        formatter: timeFormatter,
                        status: outgoingStatus.configurationStatus(isSeenByPeer: isSeenByPeer)
                    ),
                    onTap: onTap
                )
            }

        return .init(message.messageId, configuration)
    }

    func formatCallDuration(_ milliseconds: UInt64) -> String {
        let seconds = TimeInterval(milliseconds) / 1_000
        return DateComponentsFormatter.secondsMinutesAbbreviated.string(from: seconds) ?? ""
    }
}
