package uniffi.truapi

import android.os.Parcel
import android.os.Parcelable

/** A canonical product identifier validated by the Rust core. */
class ProductId private constructor(val value: String) : Parcelable {
    companion object {
        /** Validate and normalize an identifier using the core's product rules. */
        fun parse(value: String): Result<ProductId> = try {
            Result.success(parseProductId(value))
        } catch (error: ProductIdException) {
            Result.failure(error)
        }

        internal fun fromRust(value: String): ProductId = ProductId(value)

        @JvmField
        val CREATOR: Parcelable.Creator<ProductId> = object : Parcelable.Creator<ProductId> {
            override fun createFromParcel(parcel: Parcel): ProductId = parse(requireNotNull(parcel.readString())).getOrThrow()

            override fun newArray(size: Int): Array<ProductId?> = arrayOfNulls(size)
        }
    }

    override fun equals(other: Any?): Boolean = other is ProductId && value == other.value

    override fun hashCode(): Int = value.hashCode()

    override fun toString(): String = value

    override fun describeContents(): Int = 0

    override fun writeToParcel(parcel: Parcel, flags: Int) = parcel.writeString(value)
}
