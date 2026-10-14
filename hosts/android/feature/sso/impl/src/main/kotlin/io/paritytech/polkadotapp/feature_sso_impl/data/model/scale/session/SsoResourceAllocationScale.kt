package io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.session

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.FixedLength
import io.paritytech.polkadotapp.chains.util.Sr25519SecretKey
import io.paritytech.polkadotapp.feature_products_api.model.scale.ProductDerivationIndexScale
import io.paritytech.polkadotapp.feature_products_api.model.scale.ProductIdScale
import kotlinx.serialization.Serializable

@Serializable
sealed class SsoApAllocatableResourceScale {
    @Serializable
    @EnumIndex(0)
    data object StatementStoreAllowance : SsoApAllocatableResourceScale()

    @Serializable
    @EnumIndex(1)
    data object BulletInAllowance : SsoApAllocatableResourceScale()

    @Serializable
    @EnumIndex(2)
    class SmartContractAllowance(val dest: ProductDerivationIndexScale) : SsoApAllocatableResourceScale()

    @Serializable
    @EnumIndex(3)
    data object AutoSigning : SsoApAllocatableResourceScale()
}

@Serializable
sealed class SsoApAllocatedResourceScale {
    @Serializable
    @EnumIndex(0)
    class StatementStoreAllowance(val slotAccountKey: ByteArray) : SsoApAllocatedResourceScale()

    @Serializable
    @EnumIndex(1)
    class BulletInAllowance(val slotAccountKey: ByteArray) : SsoApAllocatedResourceScale()

    @Serializable
    @EnumIndex(2)
    data object SmartContractAllowance : SsoApAllocatedResourceScale()

    // RFC-0022 secret key of //product//{productId}. Shape only — nothing allocates this yet.
    @Serializable
    @EnumIndex(3)
    class AutoSigning(
        @FixedLength(Sr25519SecretKey.SIZE_BYTES)
        val productRootSecretKey: ByteArray,
    ) : SsoApAllocatedResourceScale()
}

@Serializable
sealed class SsoApAllocationOutcomeScale {
    @Serializable
    @EnumIndex(0)
    class Allocated(val resource: SsoApAllocatedResourceScale) : SsoApAllocationOutcomeScale()

    @Serializable
    @EnumIndex(1)
    data object Rejected : SsoApAllocationOutcomeScale()

    @Serializable
    @EnumIndex(2)
    data object NotAvailable : SsoApAllocationOutcomeScale()
}

@Serializable
enum class SsoOnExistingAllowancePolicyScale {
    @EnumIndex(0)
    IGNORE,

    @EnumIndex(1)
    INCREASE,
}

@Serializable
class SsoResourceAllocationRequestScale(
    val callingProductId: ProductIdScale,
    val resources: List<SsoApAllocatableResourceScale>,
    val onExisting: SsoOnExistingAllowancePolicyScale,
)
