package io.paritytech.polkadotapp.tools_ipfs_api

class IpfsFileMetadata(
    val chunks: List<String>,
    val hash: String,
    val totalSize: Long,
    val path: String
)
