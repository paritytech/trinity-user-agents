package io.paritytech.polkadotapp.tools_integrity_impl.data.interceptors

import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.mapError
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.tools_integrity_api.domain.error.IntegrityError
import io.paritytech.polkadotapp.tools_integrity_api.interceptors.BackendIntegrityInterceptor
import io.paritytech.polkadotapp.tools_integrity_impl.data.api.IntegrityApi
import io.paritytech.polkadotapp.tools_integrity_impl.data.integrity.IntegrityParamsInjector
import kotlinx.coroutines.runBlocking
import okhttp3.Request
import retrofit2.Retrofit
import javax.inject.Inject

class RealBackendIntegrityInterceptor @Inject constructor(
    private val integrityParamsInjector: IntegrityParamsInjector,
) : BackendIntegrityInterceptor() {
    lateinit var integrityApi: IntegrityApi

    override fun initRetrofit(retrofit: Retrofit) {
        this.integrityApi = retrofit.create(IntegrityApi::class.java)
    }

    override fun buildInterceptedRequest(request: Request): Result<Request> {
        return runBlocking {
            val requestBuilder = request.newBuilder()

            runCancellableCatching { integrityApi.fetchChallenge().challenge }
                // The challenge round trip is plain networking, so a failure here is retryable.
                .mapError { IntegrityError.AttestationTransient }
                .flatMap { challenge ->
                    integrityParamsInjector(requestBuilder, integrityApi, challenge)
                }
                .map { requestBuilder.build() }
                .logFailure("Failed to add integrity headers")
        }
    }
}
