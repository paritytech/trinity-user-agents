package io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.common.domain.model.DataByteArray

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

        /** Historical core bundle: keep its exact identity, especially for a bundle denial. */
        data class NetworkAccessSet(val domains: List<String>) : RemotePermission {
            override val typeName: String get() = "network_access_set"
            override val key: String get() = domains.joinToString("|")
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

        /** Stored per network; [genesis] is the lowercase `0x`-prefixed genesis hash. */
        data class JamPeersAccess(
            val genesis: String,
        ) : RemotePermission {
            override val typeName: String get() = TYPE_NAME
            override val key: String get() = genesis

            /** `0x` plus the first 8 hex digits, as shown in prompts and the permissions list. */
            val shortGenesis: String get() = genesis.take(10) + "…"

            companion object {
                const val TYPE_NAME = "jam_peers"
            }
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

    data object ChatAuthority : ProductPermission {
        const val TYPE_NAME = "chat_authority"
        override val typeName: String get() = TYPE_NAME
        override val key: String get() = ""
    }

    data object ProfileDisclosure : ProductPermission {
        const val TYPE_NAME = "profile_disclosure"
        override val typeName: String get() = TYPE_NAME
        override val key: String get() = ""
    }

    /** Null is the legacy allowance account, not product account zero. */
    data class StatementStoreAllowance(val derivationIndex: AllowanceAccountSelector?) : ProductPermission {
        override val typeName: String get() = TYPE_NAME
        override val key: String get() = when (val selector = derivationIndex) {
            null -> "legacy"
            is AllowanceAccountSelector.Index -> "index:${selector.value}"
            is AllowanceAccountSelector.Raw -> "raw:${selector.value}"
        }

        companion object {
            const val TYPE_NAME = "statement_store_allowance"
        }
    }

    companion object {
        fun fromLocal(typeName: String, key: String): ProductPermission {
            return when (typeName) {
                DeviceCapability.TYPE_NAME -> DeviceCapability(DeviceCapabilityType.valueOf(key))
                AccountAccess.TYPE_NAME -> AccountAccess(key)
                BalanceAccess.TYPE_NAME -> BalanceAccess
                UserIdentityAccess.TYPE_NAME -> UserIdentityAccess
                ChatAuthority.TYPE_NAME -> ChatAuthority
                ProfileDisclosure.TYPE_NAME -> ProfileDisclosure
                StatementStoreAllowance.TYPE_NAME -> StatementStoreAllowance(
                    when {
                        key == "legacy" -> null
                        key.startsWith("index:") -> AllowanceAccountSelector.Index(key.removePrefix("index:").toUInt())
                        key.startsWith("raw:") -> AllowanceAccountSelector.Raw(DataByteArray.fromHex(key.removePrefix("raw:")))
                        else -> error("Unknown allowance account selector: $key")
                    }
                )
                RemotePermission.NetworkAccess.TYPE_NAME -> RemotePermission.NetworkAccess(key)
                RemotePermission.WebRtcAccess.TYPE_NAME -> RemotePermission.WebRtcAccess
                RemotePermission.ChainSubmitAccess.TYPE_NAME -> RemotePermission.ChainSubmitAccess
                RemotePermission.StatementSubmitAccess.TYPE_NAME -> RemotePermission.StatementSubmitAccess
                RemotePermission.PreimageSubmitAccess.TYPE_NAME -> RemotePermission.PreimageSubmitAccess
                RemotePermission.JamPeersAccess.TYPE_NAME -> RemotePermission.JamPeersAccess(key)
                else -> error("Unknown permission type: $typeName")
            }
        }
    }
}

/** Preserve the canonical enum tag as well as its bytes; Raw is not an Index alias. */
sealed interface AllowanceAccountSelector {
    data class Index(val value: UInt) : AllowanceAccountSelector
    data class Raw(val value: DataByteArray) : AllowanceAccountSelector {
        init {
            require(value.value.size == 32) { "Raw allowance account selector must be 32 bytes" }
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
