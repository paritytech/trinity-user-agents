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
            clearRoomFooterCard(of: metadata.chatId)
            return makeTextInputConfiguration(inputFieldMetadata)
        case .incomingRequest:
            return makeIncomingRequestInputConfiguration(for: metadata.peerMetadata, actions: actions)
        case .outgoingRequest:
            return makeOutgoingPendingRequestInputConfiguration(for: metadata)
        case .blockedUser:
            return makeBlockedUserInputConfiguration(for: metadata.peerMetadata, actions: actions)
        case let .productCard(roomId, messageType, payload):
            return productMessageDecoder?
                .makeRoomFooterCard(roomId: roomId, messageType: messageType, payload: payload)
        case .empty:
            clearRoomFooterCard(of: metadata.chatId)
            return nil
        }
    }
}

private extension ChatViewModelFactory {
    var productMessageDecoder: ProductMessageDecoder? {
        customDecodersById[MessageDecoderIdentifier.product.rawValue] as? ProductMessageDecoder
    }

    func clearRoomFooterCard(of chatId: Chat.Id) {
        guard case let .chatExtension(_, roomId?) = chatId else { return }
        productMessageDecoder?.clearRoomFooterCard(roomId: roomId)
    }

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
