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
            onTap: onTap
        )
        return ChatMessageContainerConfiguration(
            innerContent: configuration,
            side: .leading,
            bubbleColor: .bgSurfaceContainer,
            statusConfiguration: statusConfiguration,
            canReply: false,
            contentInsets: .zero,
            statusViewInsets: .init(bottom: DSSpacings.extraSmall, right: DSSpacings.small),
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
            onTap: onTap
        )
        return ChatMessageContainerConfiguration(
            innerContent: configuration,
            side: .trailing,
            bubbleColor: .bgSurfaceContainerInverted,
            statusConfiguration: statusConfiguration,
            canReply: false,
            contentInsets: .zero,
            statusViewInsets: .init(bottom: DSSpacings.extraSmall, right: DSSpacings.small),
            identifier: ChatCallMessageConfiguration.defaultReuseIdentifier
        )
    }
}
