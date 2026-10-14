package io.paritytech.polkadotapp.feature_chats_impl.domain.sessions

import io.paritytech.polkadotapp.feature_statement_store_api.domain.CommunicationSession

enum class ContactSessionRole {
    Combined,
    MultiDevice,
    Identity,
}

data class RoledCommunicationSession(
    val session: CommunicationSession,
    val role: ContactSessionRole,
)

sealed interface CommunicationSessions {
    val main: CommunicationSession
    val identity: CommunicationSession

    val distinct: Set<RoledCommunicationSession>

    class MultiDevice(
        override val main: CommunicationSession,
        override val identity: CommunicationSession,
    ) : CommunicationSessions {
        override val distinct: Set<RoledCommunicationSession> = setOf(
            RoledCommunicationSession(main, ContactSessionRole.MultiDevice),
            RoledCommunicationSession(identity, ContactSessionRole.Identity),
        )
    }

    class Pairwise(session: CommunicationSession) : CommunicationSessions {
        override val main: CommunicationSession = session
        override val identity: CommunicationSession = session
        override val distinct: Set<RoledCommunicationSession> = setOf(
            RoledCommunicationSession(session, ContactSessionRole.Combined),
        )
    }
}
