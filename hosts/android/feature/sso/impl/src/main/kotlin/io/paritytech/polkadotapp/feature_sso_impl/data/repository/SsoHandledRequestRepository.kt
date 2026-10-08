package io.paritytech.polkadotapp.feature_sso_impl.data.repository

import io.paritytech.polkadotapp.database.dao.SsoHandledRequestDao
import io.paritytech.polkadotapp.database.model.SsoHandledRequestLocal
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionId
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionRequest
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionRequestId
import javax.inject.Inject

class SsoHandledRequestRepository @Inject constructor(
    private val ssoHandledRequestDao: SsoHandledRequestDao,
) {
    suspend fun wasHandled(request: SsoSessionRequest): Boolean = wasHandled(request.sessionId, request.requestId)

    suspend fun wasHandled(sessionId: SsoSessionId, requestId: SsoSessionRequestId): Boolean {
        return ssoHandledRequestDao.isHandled(sessionId.value, requestId)
    }

    suspend fun markHandled(request: SsoSessionRequest) {
        ssoHandledRequestDao.insert(
            SsoHandledRequestLocal(sessionId = request.sessionId.value, requestId = request.requestId)
        )
    }
}
