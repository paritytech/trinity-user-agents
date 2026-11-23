package io.paritytech.polkadotapp.feature_products_api.presentation.widget.materials

import uniffi.truapi.ImageRef
import uniffi.truapi.ImageSource
import uniffi.truapi.Parameter
import uniffi.truapi.ReadyLayer

/**
 * Whether two layer lists draw the same. The generated layer compares its material and inline images by
 * reference, and the core sends new ones on every update, so equal layers are compared by content.
 */
fun List<ReadyLayer>.drawsSameAs(other: List<ReadyLayer>): Boolean =
    size == other.size && zip(other).all { (mine, theirs) -> mine.drawsSameAs(theirs) }

/** The image a sampler argument reads, or null for a numeric argument. */
fun Parameter.imageRef(): ImageRef? = when (this) {
    is Parameter.SamplerFloat -> v1
    is Parameter.SamplerVec2 -> v1
    is Parameter.SamplerVec3 -> v1
    is Parameter.SamplerVec4 -> v1
    is Parameter.Float, is Parameter.Vec2, is Parameter.Vec3, is Parameter.Vec4 -> null
}

private fun ReadyLayer.drawsSameAs(other: ReadyLayer): Boolean =
    material.contentEquals(other.material) && shader == other.shader &&
        args.size == other.args.size && args.zip(other.args).all { (mine, theirs) -> mine.sameAs(theirs) }

private fun Parameter.sameAs(other: Parameter): Boolean {
    val mine = (imageRef() as? ImageRef.Source)?.v1 as? ImageSource.Inline
    val theirs = (other.imageRef() as? ImageRef.Source)?.v1 as? ImageSource.Inline
    return if (mine != null && theirs != null) mine.v1.contentEquals(theirs.v1) else this == other
}
