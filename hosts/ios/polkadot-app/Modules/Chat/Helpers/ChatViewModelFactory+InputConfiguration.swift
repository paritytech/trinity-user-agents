import ExternalAccessibility
import Foundation
import PolkadotUI

extension ChatViewModelFactory {
    func makeInputConfiguration(
        for metadata: ChatMetadata,
        actions: ChatViewModelActions
    ) -> (any ChatInputViewConfigurationProtocol)? {
        switch metadata.peerMetadata.input {
        case let .inputField(inputFieldMetadata):
            makeTextInputConfiguration(inputFieldMetadata)
        case .incomingRequest:
            makeIncomingRequestInputConfiguration(for: metadata.peerMetadata, actions: actions)
        case .outgoingRequest:
            makeOutgoingPendingRequestInputConfiguration(for: metadata)
        case .blockedUser:
            makeBlockedUserInputConfiguration(for: metadata.peerMetadata, actions: actions)
        case .empty:
            nil
        }
    }
}

private extension ChatViewModelFactory {
    func makeTextInputConfiguration(
        _ inputFieldMetadata: Chat.PeerMetadataInput.InputField
    ) -> any ChatInputViewConfigurationProtocol {
        ChatInputViewConfiguration.chat(
            canPay: inputFieldMetadata.canPay,
            canAttachFile: inputFieldMetadata.canAttachFile
        )
    }

    func makeIncomingRequestInputConfiguration(
        for peer: Chat.PeerMetadata,
        actions: ChatViewModelActions
    ) -> any ChatInputViewConfigurationProtocol {
        ChatAcceptBannerView.ViewModel(
            username: peer.name,
            onDecline: actions.declineChatRequest,
            onAccept: actions.acceptChatRequest
        )
    }

    func makeBlockedUserInputConfiguration(
        for peer: Chat.PeerMetadata,
        actions: ChatViewModelActions
    ) -> any ChatInputViewConfigurationProtocol {
        ChatBlockedBannerView.ViewModel(
            username: peer.name,
            onUnblock: actions.unblockUser
        )
    }

    func makeOutgoingPendingRequestInputConfiguration(
        for metadata: ChatMetadata
    ) -> any ChatInputViewConfigurationProtocol {
        switch metadata.state {
        case .created:
            ChatRequestedBannerView.ViewModel(
                username: metadata.peerMetadata.name
            )
        case .pending:
            ChatRequestInProgressBannerView.ViewModel(
                username: metadata.peerMetadata.name,
                inputConfig: .chatRequest(
                    inputAccessibilityId: AccessibilityID.Chats.inviteMessageInput,
                    sendAccessibilityId: AccessibilityID.Chats.inviteSendButton
                )
            )
        }
    }
}
