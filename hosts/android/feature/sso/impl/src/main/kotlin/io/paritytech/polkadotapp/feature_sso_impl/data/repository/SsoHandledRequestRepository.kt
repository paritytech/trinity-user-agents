package io.paritytech.polkadotapp.feature_sso_impl.data.repository

import io.paritytech.polkadotapp.database.dao.SsoHandledRequestDao
import io.paritytech.polkadotapp.database.model.SsoHandledRequestLocal
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionRequest
import javax.inject.Inject

class SsoHandledRequestRepository @Inject constructor(
    private val ssoHandledRequestDao: SsoHandledRequestDao,
) {
    suspend fun wasHandled(request: SsoSessionRequest): Boolean {
        return ssoHandledRequestDao.isHandled(request.sessionId.value, request.requestId)
    }

    suspend fun markHandled(request: SsoSessionRequest) {
        ssoHandledRequestDao.insert(
            SsoHandledRequestLocal(sessionId = request.sessionId.value, requestId = request.requestId)
        )
    }
}
