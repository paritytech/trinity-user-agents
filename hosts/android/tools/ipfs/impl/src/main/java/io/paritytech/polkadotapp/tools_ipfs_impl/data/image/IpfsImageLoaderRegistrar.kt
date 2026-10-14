package io.paritytech.polkadotapp.tools_ipfs_impl.data.image

import coil.ComponentRegistry
import com.google.gson.Gson
import io.paritytech.polkadotapp.common.data.image.ImageLoaderComponentRegistrar
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsContentLookup
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsImageRequest
import io.paritytech.polkadotapp.tools_ipfs_impl.data.IpfsImageFetcher
import okhttp3.OkHttpClient
import javax.inject.Inject

internal class IpfsImageLoaderRegistrar @Inject constructor(
    private val okHttpClient: OkHttpClient,
    private val gson: Gson,
    private val ipfsContentLookup: IpfsContentLookup,
) : ImageLoaderComponentRegistrar {
    override fun register(builder: ComponentRegistry.Builder) {
        builder.add(
            IpfsImageFetcher.Factory(okHttpClient, gson, ipfsContentLookup),
            IpfsImageRequest::class.java,
        )
    }
}
