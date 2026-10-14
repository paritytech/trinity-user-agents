import Foundation
import PolkadotUI

protocol ShareViewModelFactoryProtocol {
    func createViewModel(
        chats: [ChatWithPeerMetadata],
        selectedIds: Set<Chat.Id>,
        isLoading: Bool,
        onSelection: @escaping (Chat.Id, Bool) -> Void
    ) -> ShareViewLayout.ViewModel
}

final class ShareViewModelFactory: ShareViewModelFactoryProtocol {
    func createViewModel(
        chats: [ChatWithPeerMetadata],
        selectedIds: Set<Chat.Id>,
        isLoading: Bool,
        onSelection: @escaping (Chat.Id, Bool) -> Void
    ) -> ShareViewLayout.ViewModel {
        let contacts = chats.map { chatWithMetadata in
            let chat = chatWithMetadata.chat
            let peerMetadata = chatWithMetadata.peerMetadata

            let configuration = SelectableContactConfiguration(
                avatar: makeAvatarViewModel(chat: chat, peerMetadata: peerMetadata),
                name: peerMetadata.name,
                isSelected: selectedIds.contains(chat.chatId),
                onSelection: { newValue in
                    onSelection(chat.chatId, newValue)
                }
            )
            return IdentifiableContentConfiguration(
                id: chat.identifier,
                configuration: configuration
            )
        }

        let hasSelection = !selectedIds.isEmpty

        return ShareViewLayout.ViewModel(
            contacts: contacts,
            isShareVisible: hasSelection,
            isLoading: isLoading
        )
    }

    private func makeAvatarViewModel(
        chat: Chat.LocalModel,
        peerMetadata: Chat.PeerMetadata
    ) -> AvatarViewModel {
        if let image = peerMetadata.icon.image {
            return .image(image)
        }
        let prefix = String(peerMetadata.name.prefix(1))
        return .colored(text: prefix, colorSeed: chat.chatId.colorSeed)
    }
}
