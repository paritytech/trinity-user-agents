package uniffi.truapi

/** A canonical product identifier validated by the Rust core. */
class ProductId private constructor(val value: String) {
    companion object {
        /** Validate and normalize an identifier using the core's product rules. */
        fun parse(value: String): Result<ProductId> = try {
            Result.success(parseProductId(value))
        } catch (error: ProductIdException) {
            Result.failure(error)
        }

        internal fun fromRust(value: String): ProductId = ProductId(value)
    }

    override fun equals(other: Any?): Boolean = other is ProductId && value == other.value

    override fun hashCode(): Int = value.hashCode()

    override fun toString(): String = value
}
