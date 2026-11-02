package io.paritytech.polkadotapp.feature_statement_store_api.domain

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_statement_store_api.data.encryption.CommunicationEncryption
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.EncodedMessage
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.SessionAccount

/**
 * Puts a single request on the outgoing topic of a pairwise [CommunicationSession] without running the session, for
 * statements that must be signed by an account the session does not own. The peer's session receives it like any
 * other request; nothing here waits for, or expects, a response.
 */
interface OneShotSessionRequestPublisher {
    /** Submits [messages] as request [requestId], signed by [prover]. Re-publishing the same id is a renewal. */
    suspend fun publish(
        localAccount: SessionAccount.Local,
        remoteAccount: SessionAccount.Remote,
        encryption: CommunicationEncryption,
        requestId: RequestId,
        messages: List<EncodedMessage>,
        prover: StatementStoreMessageProver,
    ): Result<Unit>

    /** Whether a request signed by [signer] is still stored on the session's outgoing topic. */
    suspend fun isStoredBy(
        localAccount: SessionAccount.Local,
        remoteAccount: SessionAccount.Remote,
        encryption: CommunicationEncryption,
        signer: AccountId,
    ): Result<Boolean>
}
