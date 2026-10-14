package io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models

import androidx.compose.runtime.Immutable

@Immutable
sealed interface ProductPermission {
    val typeName: String
    val key: String

    sealed interface RemotePermission : ProductPermission {
        /**
         * Stored per-domain; multi-domain Remote(Vec<String>) DTOs are exploded at the bridge.
         * Group identity is intentionally not retained in the domain model.
         */
        data class NetworkAccess(
            val domain: String,
        ) : RemotePermission {
            override val typeName: String get() = TYPE_NAME
            override val key: String get() = domain

            companion object {
                const val TYPE_NAME = "network_access"
            }
        }

        data object WebRtcAccess : RemotePermission {
            const val TYPE_NAME = "webrtc_access"
            override val typeName: String get() = TYPE_NAME
            override val key: String get() = ""
        }

        data object ChainSubmitAccess : RemotePermission {
            const val TYPE_NAME = "chain_submit"
            override val typeName: String get() = TYPE_NAME
            override val key: String get() = ""
        }

        data object StatementSubmitAccess : RemotePermission {
            const val TYPE_NAME = "statement_submit"
            override val typeName: String get() = TYPE_NAME
            override val key: String get() = ""
        }

        data object PreimageSubmitAccess : RemotePermission {
            const val TYPE_NAME = "preimage_submit"
            override val typeName: String get() = TYPE_NAME
            override val key: String get() = ""
        }
    }

    data class DeviceCapability(
        val capability: DeviceCapabilityType,
    ) : ProductPermission {
        override val typeName: String get() = TYPE_NAME
        override val key: String get() = capability.name

        companion object {
            const val TYPE_NAME = "device_capability"
        }
    }

    data class AccountAccess(
        val targetProductId: String,
    ) : ProductPermission {
        override val typeName: String get() = TYPE_NAME
        override val key: String get() = targetProductId

        companion object {
            const val TYPE_NAME = "account_access"
        }
    }

    data object BalanceAccess : ProductPermission {
        const val TYPE_NAME = "balance_access"
        override val typeName: String get() = TYPE_NAME
        override val key: String get() = ""
    }

    data object UserIdentityAccess : ProductPermission {
        const val TYPE_NAME = "user_identity_access"
        override val typeName: String get() = TYPE_NAME
        override val key: String get() = ""
    }

    companion object {
        fun fromLocal(typeName: String, key: String): ProductPermission {
            return when (typeName) {
                DeviceCapability.TYPE_NAME -> DeviceCapability(DeviceCapabilityType.valueOf(key))
                AccountAccess.TYPE_NAME -> AccountAccess(key)
                BalanceAccess.TYPE_NAME -> BalanceAccess
                UserIdentityAccess.TYPE_NAME -> UserIdentityAccess
                RemotePermission.NetworkAccess.TYPE_NAME -> RemotePermission.NetworkAccess(key)
                RemotePermission.WebRtcAccess.TYPE_NAME -> RemotePermission.WebRtcAccess
                RemotePermission.ChainSubmitAccess.TYPE_NAME -> RemotePermission.ChainSubmitAccess
                RemotePermission.StatementSubmitAccess.TYPE_NAME -> RemotePermission.StatementSubmitAccess
                RemotePermission.PreimageSubmitAccess.TYPE_NAME -> RemotePermission.PreimageSubmitAccess
                else -> error("Unknown permission type: $typeName")
            }
        }
    }
}

data class ProductPermissionStatus(
    val permission: ProductPermission,
    val granted: Boolean
)

enum class DeviceCapabilityType {
    Notifications,
    Camera,
    Microphone,
    Bluetooth,
    NFC,
    Location,
    Clipboard,
    Biometrics,
    OpenUrl,
}
