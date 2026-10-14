import Foundation
import PolkadotUI

// MARK: - Call bubble rendering

extension ChatViewModelFactory {
    func callConfigurations(
        for payload: Chat.LocalMessage.Content.CallSignalingPayload,
        message: Chat.LocalMessage,
        status: Chat.LocalMessage.Status,
        deliveryDate: Date?,
        actions: ChatViewModelActions
    ) -> [IdentifiableAnyContentConfiguration<ChatViewLayout.ItemIdentifierType>] {
        switch payload {
        case let .offer(offerContent):
            [
                callMessageConfiguration(
                    messageId: message.messageId,
                    offer: offerContent,
                    status: status,
                    callState: message.resolveCallState() ?? .calling,
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
        messageId: String,
        offer: Chat.RemoteMessageContentV1.MessageContent.DataChannelOfferContent,
        status: Chat.LocalMessage.Status,
        callState: Chat.CallState,
        deliveryDate: Date?,
        actions: ChatViewModelActions
    ) -> IdentifiableAnyContentConfiguration<ChatViewLayout.ItemIdentifierType> {
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
            case let .cancelled(duration):
                .cancelled(ringDuration: formatCallDuration(duration))
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
                        status: outgoingStatus.configurationStatus()
                    ),
                    onTap: onTap
                )
            }

        return .init(messageId, configuration)
    }

    func formatCallDuration(_ milliseconds: UInt64) -> String {
        let seconds = TimeInterval(milliseconds) / 1_000
        return DateComponentsFormatter.secondsMinutesAbbreviated.string(from: seconds) ?? ""
    }
}
