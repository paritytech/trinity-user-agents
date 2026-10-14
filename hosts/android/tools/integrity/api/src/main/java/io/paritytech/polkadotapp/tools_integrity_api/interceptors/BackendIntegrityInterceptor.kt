package io.paritytech.polkadotapp.tools_integrity_api.interceptors

import retrofit2.Retrofit

abstract class BackendIntegrityInterceptor : IntegrityInterceptor() {
    abstract fun initRetrofit(retrofit: Retrofit)
}
