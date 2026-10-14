package io.paritytech.polkadotapp.database.model

import androidx.room.Entity

/**
 * RFC-0024 ring VRF key registry. One row per declared ring, so registering an already-known index
 * for a further ring is an insert rather than a read-modify-write.
 *
 * [ringLocation] is the SCALE-encoded ring, opaque here — ring equality is byte equality.
 */
@Entity(
    tableName = "ring_vrf_key_registrations",
    primaryKeys = ["ownerProductId", "derivationIndex", "ringLocation"]
)
class RingVrfKeyRegistrationLocal(
    val ownerProductId: String,
    val derivationIndex: ByteArray,
    val ringLocation: ByteArray,
    val publicKey: ByteArray,
    val registeredAt: Long,
)
