package io.paritytech.polkadotapp.tools_integrity_api.interceptors

import okhttp3.Interceptor

/** Adds Widevine claim evidence and handles evidence-specific backend retries. */
interface WidevineIntegrityInterceptor : Interceptor
