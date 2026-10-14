package io.paritytech.polkadotapp.feature_statement_store_impl.data.models.scale

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.FixedLength
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.ByteArraySerializable
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import kotlinx.serialization.Serializable

/**
 * Per-recipient-device entry inside a multi-device envelope. [encryptedKey] is the wrapped
 * symmetric key — only the device with [statementAccountId] can decrypt it.
 *
 * `statementAccountId` is a bare `ByteArray`, not `AccountId` (= DataByteArray) — the
 * binary scale encoder loses `@FixedLength` annotations when descending into a struct
 * wrapper, so wrapping the field in `DataByteArray` would silently emit a compact-length
 * prefix on the wire and break interop with the spec'd 32-raw-byte format.
 */
@Serializable
class RequestDeviceInfo(
    @FixedLength(32) val statementAccountId: ByteArraySerializable,
    val encryptedKey: DataByteArray
)
