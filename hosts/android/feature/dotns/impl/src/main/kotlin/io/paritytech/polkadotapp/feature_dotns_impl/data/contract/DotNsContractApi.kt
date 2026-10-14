package io.paritytech.polkadotapp.feature_dotns_impl.data.contract

interface DotNsContractApi {
    suspend fun resolveContentHash(dotNsName: String): Result<ByteArray?>

    suspend fun getMetadata(dotNsName: String, key: String): Result<String?>
}
