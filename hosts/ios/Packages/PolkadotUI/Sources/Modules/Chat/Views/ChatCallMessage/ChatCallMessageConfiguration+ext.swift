import UIKit
import DesignSystem

public extension ChatCallMessageConfiguration {
    static func inbox(
        callType: CallType,
        state: State,
        statusConfiguration: ChatMessageStatusViewConfiguration,
        onTap: (() -> Void)? = nil
    ) -> ChatMessageContainerConfiguration {
        let configuration = ChatCallMessageConfiguration(
            callType: callType,
            direction: .incoming,
            state: state,
            statusConfiguration: statusConfiguration,
            onTap: onTap
        )
        return ChatMessageContainerConfiguration(
            innerContent: configuration,
            side: .leading,
            bubbleColor: .bgSurfaceContainer,
            canReply: false,
            contentInsets: .zero,
            identifier: ChatCallMessageConfiguration.defaultReuseIdentifier
        )
    }

    static func outbox(
        callType: CallType,
        state: State,
        statusConfiguration: ChatMessageStatusViewConfiguration,
        onTap: (() -> Void)? = nil
    ) -> ChatMessageContainerConfiguration {
        let configuration = ChatCallMessageConfiguration(
            callType: callType,
            direction: .outgoing,
            state: state,
            statusConfiguration: statusConfiguration,
            onTap: onTap
        )
        return ChatMessageContainerConfiguration(
            innerContent: configuration,
            side: .trailing,
            bubbleColor: .bgSurfaceContainerInverted,
            canReply: false,
            contentInsets: .zero,
            identifier: ChatCallMessageConfiguration.defaultReuseIdentifier
        )
    }
}
