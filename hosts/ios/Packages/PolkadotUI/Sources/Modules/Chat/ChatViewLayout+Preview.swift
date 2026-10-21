#if DEBUG
    import SwiftUI
    import UIKit

    #Preview {
        let message1 = ChatMessageContainerConfiguration.inboxRichText(
            text: "Just wanted to say hi, how’s your day going? Just wanted to say hi, how’s your day going? Just wanted to say hi, how’s your day going?",
            statusConfiguration: nil
        )

        let message2 = ChatMessageContainerConfiguration.outboxRichText(
            text: "Hi! I’m doing well, thanks for asking. And you? How about you?",
            statusConfiguration: .read
        )

        let message3 = ChatMessageContainerConfiguration.inboxRichText(
            text: "That’s awesome! Mine’s going okay, just getting through the week. How about you? Anything exciting planned for the weekend?",
            statusConfiguration: nil
        )

        let message4 = ChatMessageContainerConfiguration.inboxRichText(
            text: "Oh, I’ve got a couple of things planned. I’m going for a hike this weekend and then catching up with some friends. How about you? Any plans of your own?",
            statusConfiguration: .read
        )

        let textImage = ChatMessageContainerConfiguration.botTextImage(
            text: "Oh, I’ve got a couple of things planned. I’m going for a hike this weekend and then catching up with",
            image: UIImage(systemName: "1.circle.fill")!
        )
        let imageOnly = ChatMessageContainerConfiguration.botTextImage(
            text: nil,
            image: UIImage(systemName: "1.circle.fill")!
        )

        let layout = ChatViewLayout()
        let systemMessages: [any HashableContentConfiguration] = [
            ChatInfoMessageConfiguration.youAdded(username: "username.77"),
            ChatInfoMessageConfiguration.youAdded(by: "username.88"),
            ChatInfoMessageConfiguration.chatRequested(),
            ChatInfoMessageConfiguration.chatRequestAccepted(by: "username.88"),
            ChatInfoMessageConfiguration.newMessages(),
            message1,
            message2,
            message3,
            message4,
            textImage,
            imageOnly
        ]
        let msgs: [IdentifiableAnyContentConfiguration<String>] = systemMessages.map {
            IdentifiableAnyContentConfiguration(UUID().uuidString, $0)
        }

        let viewModel = ChatViewLayout.ViewModel(
            headerConfiguration: ChatHeaderConfiguration(
                avatarViewModel: .colored(text: "U", colorSeed: "test"),
                username: "username"
            ),
            chatInputConfiguration: ChatInputViewConfiguration.chat(canPay: true, canAttachFile: true),
            scrollDownConfiguration: .init(available: true, unreadCount: 1),
            sections: [.init(identifier: "Section 1", dateText: "Today", messages: msgs)],
            footerConfiguration: nil
        )
        layout.bind(viewModel: viewModel)

        let viewController = UIViewController()
        viewController.view.addSubview(layout)
        layout.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }

        return viewController
    }
#endif
