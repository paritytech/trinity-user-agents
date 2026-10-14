package io.paritytech.polkadotapp.tools_integrity_api.interceptors

import okhttp3.Interceptor
import okhttp3.Request
import okhttp3.Response
import retrofit2.Invocation
import java.io.IOException

abstract class IntegrityInterceptor : Interceptor {
    abstract fun buildInterceptedRequest(request: Request): Result<Request>

    override fun intercept(chain: Interceptor.Chain): Response {
        val request = chain.request()
        if (!checkIfNeedToIntercept(chain)) return chain.proceed(request)

        // The Result -> OkHttp seam: an Interceptor may only throw IOException, so the typed
        // cause rides inside one. A header-less request is never accepted by the backend, so
        // failing here costs nothing but surfaces the real reason.
        val interceptedRequest = buildInterceptedRequest(request)
            .getOrElse { throw IOException("Failed to attach integrity headers", it) }

        return chain.proceed(interceptedRequest)
    }

    private fun checkIfNeedToIntercept(chain: Interceptor.Chain): Boolean {
        val request = chain.request()
        val invocation = request.tag(Invocation::class.java)
        return CallWithIntegrity() in (invocation?.method()?.annotations ?: arrayOf())
    }
}
