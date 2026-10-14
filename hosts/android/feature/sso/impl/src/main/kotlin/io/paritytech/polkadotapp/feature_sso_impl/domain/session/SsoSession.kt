package io.paritytech.polkadotapp.feature_sso_impl.domain.session

import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.mapError
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.session.decodeAlwaysDecodableSsoMessagePart
import io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.session.toEncodedMessage
import io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.session.toSsoSessionRequest
import io.paritytech.polkadotapp.feature_sso_impl.domain.model.SsoSessionData
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SendMessageError
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionRequest
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionRequestId
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionResponse
import io.paritytech.polkadotapp.feature_statement_store_api.domain.CommunicationSession
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.CommunicationSessionEvent
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.EncodedMessage
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import timber.log.Timber

class SsoCommunicationSession(
    scope: CoroutineScope,
    private val session: SsoSessionData,
    private val communicationSession: CommunicationSession,
    private val dotNsTldProvider: DotNsTldProvider,
) : CoroutineScope by scope {
    val id = session.id

    val sessionData: SsoSessionData
        get() = session

    private val _requests = MutableSharedFlow<SsoSessionRequest>()
    val requests: Flow<SsoSessionRequest> = _requests.asSharedFlow()

    private val sentMessagesStream = MutableSharedFlow<SentMessageOutcome>(replay = 1, extraBufferCapacity = 10, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    init {
        subscribeSessionEvents()
    }

    private fun subscribeSessionEvents() {
        communicationSession
            .subscribeEvents()
            .onEach { event ->
                when (event) {
                    is CommunicationSessionEvent.NewMessagesReceived -> {
                        handleNewMessagesReceived(event.messages)
                    }

                    is CommunicationSessionEvent.SessionFailed -> {
                        Timber.e(event.error, "SSO session failed for ${session.name}")
                    }

                    is CommunicationSessionEvent.MessagesSentSuccessfully -> {
                        emitSentOutcomes(event.messages, Result.success(Unit))
                    }

                    is CommunicationSessionEvent.MessagesFailedToSend -> {
                        emitSentOutcomes(event.messages, Result.failure(SendMessageError.SubmissionFailed(event.error)))
                    }

                    is CommunicationSessionEvent.MessageIsTooLarge -> {
                        val requestedSize = event.message.size.bytes
                        emitSentOutcome(event.message, Result.failure(SendMessageError.MessageTooLarge(requestedSize, event.maxAllowedSize)))
                    }

                    else -> {
                        // Other events not needed for SSO
                    }
                }
            }
            .launchIn(this)
    }

    private suspend fun emitSentOutcomes(messages: List<EncodedMessage>, result: Result<Unit>) {
        messages.forEach { emitSentOutcome(it, result) }
    }

    private suspend fun emitSentOutcome(message: EncodedMessage, result: Result<Unit>) {
        message.decodeAlwaysDecodableSsoMessagePart()
            .onSuccess { decodableSsoMessagePart ->
                sentMessagesStream.emit(SentMessageOutcome(decodableSsoMessagePart.id, result))
            }
    }

    private suspend fun handleNewMessagesReceived(messages: List<EncodedMessage>) {
        for (message in messages) {
            dotNsTldProvider.getTld()
                .flatMap { tld -> message.toSsoSessionRequest(session.id, tld) }
                .onSuccess { request -> _requests.emit(request) }
                .onFailure { error ->
                    Timber.e(error, "Failed to decode SSO message for session ${session.name}")
                }
        }
    }

    suspend fun sendRequestAndAwaitSent(request: SsoSessionRequest): Result<Unit> {
        return initiateRequestSend(request)
            .mapError(SendMessageError::SubmissionFailed)
            .flatMap { awaitSent(request.requestId) }
    }

    suspend fun sendResponseAndAwaitSent(response: SsoSessionResponse): Result<Unit> {
        return initiateResponseSend(response)
            .mapError(SendMessageError::SubmissionFailed)
            .flatMap { awaitSent(response.ownRequestId) }
    }

    fun dispose() {
        cancel()
    }

    private fun initiateRequestSend(request: SsoSessionRequest): Result<Unit> {
        return runCatching {
            val encodedMessage = request.toEncodedMessage()
            communicationSession.sendMessage(encodedMessage)
        }
    }

    private fun initiateResponseSend(response: SsoSessionResponse): Result<Unit> {
        return runCatching {
            val encodedMessage = response.toEncodedMessage()
            communicationSession.sendMessage(encodedMessage)
        }
    }

    private suspend fun awaitSent(requestId: SsoSessionRequestId): Result<Unit> {
        val outcome = sentMessagesStream.first { it.requestId == requestId }
        return outcome.result
    }

    private class SentMessageOutcome(
        val requestId: SsoSessionRequestId,
        val result: Result<Unit>,
    )
}
