package io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.session

import io.paritytech.polkadotapp.feature_products_api.model.scale.ProductAccountIdScale
import kotlinx.serialization.Serializable

/**
 * SCALE-serializable representation of SignerPayloadJSON from Polkadot.js.
 * Used for signing transaction requests via SSO.
 */
@Serializable
class SsoSignerPayloadJsonScale(
    val account: ProductAccountIdScale,
    /** The checkpoint hash of the block */
    val blockHash: ByteArray,
    /** The checkpoint block number */
    val blockNumber: ByteArray,
    /** The era for this transaction */
    val era: ByteArray,
    /** The genesis hash of the chain */
    val genesisHash: ByteArray,
    /** The encoded method (with arguments) */
    val method: ByteArray,
    /** The nonce for this transaction */
    val nonce: ByteArray,
    /** The current spec version for the runtime */
    val specVersion: ByteArray,
    /** The tip for this transaction */
    val tip: ByteArray,
    /** The current transaction version for the runtime */
    val transactionVersion: ByteArray,
    /** The applicable signed extensions for this runtime */
    val signedExtensions: List<String>,
    /** The version of the extrinsic we are dealing with */
    val version: Int,
    /** The id of the asset used to pay fees (optional) */
    val assetId: ByteArray? = null,
    /** The metadataHash for the CheckMetadataHash SignedExtension (optional) */
    val metadataHash: ByteArray? = null,
    /** The mode for the CheckMetadataHash SignedExtension (optional) */
    val mode: Int? = null,
    /** Optional flag that enables the use of the signedTransaction field */
    val withSignedTransaction: Boolean? = null,
)
