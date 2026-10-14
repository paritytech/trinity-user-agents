package io.paritytech.polkadotapp.common.data.network

import com.google.gson.Gson
import io.paritytech.polkadotapp.common.BuildConfig
import okhttp3.OkHttpClient
import okhttp3.logging.HttpLoggingInterceptor
import retrofit2.Retrofit
import retrofit2.converter.gson.GsonConverterFactory
import retrofit2.converter.scalars.ScalarsConverterFactory

interface NetworkApiCreator {
    fun <T> create(service: Class<T>, baseUrl: String? = null): T

    fun createRetrofit(baseUrl: String? = null, customOkHttpClient: OkHttpClient? = null): Retrofit
}

internal class RealNetworkApiCreator(
    private val okHttpClient: OkHttpClient,
    private val gson: Gson
) : NetworkApiCreator {
    override fun <T> create(
        service: Class<T>,
        baseUrl: String?,
    ): T {
        return createRetrofit(baseUrl).create(service)
    }

    override fun createRetrofit(baseUrl: String?, customOkHttpClient: OkHttpClient?): Retrofit {
        // When no explicit baseUrl is given the call targets the identity backend; its real host is
        // resolved per-request from remote config by IdentityBackendUrlInterceptor.
        return Retrofit.Builder()
            .client(customOkHttpClient ?: okHttpClient)
            .baseUrl(baseUrl ?: IDENTITY_BACKEND_SENTINEL_URL)
            .addConverterFactory(ScalarsConverterFactory.create())
            .addConverterFactory(GsonConverterFactory.create(gson))
            .build()
    }
}

inline fun <reified T> NetworkApiCreator.create(baseUrl: String? = null): T {
    return create(T::class.java, baseUrl)
}

fun OkHttpClient.Builder.addDebugLoggingInterceptor(): OkHttpClient.Builder {
    if (BuildConfig.DEBUG) {
        addInterceptor(HttpLoggingInterceptor().setLevel(HttpLoggingInterceptor.Level.BODY))
    }
    return this
}
