package uniffi.truapi

/** A canonical product identifier validated by the Rust core. */
class ProductId private constructor(val value: String) {
    companion object {
        /** Validate and normalize an identifier using the core's product rules. */
        fun parse(value: String): Result<ProductId> = productIdResult { parseProductId(value) }

        /** Construct a product name on an explicitly supplied network. */
        fun fromName(name: String, networkSuffix: String): Result<ProductId> =
            productIdResult { productIdFromName(name, networkSuffix) }

        internal fun fromRust(value: String): ProductId = ProductId(value)
    }

    /** Name an executable within this product's namespace. */
    fun executable(kind: ExecutableKind): Result<ProductId> = productIdResult { productIdExecutable(this, kind) }

    /** Resolve an executable identity to its owning product. */
    fun owner(): Result<ProductId> = productIdResult { productIdOwner(this) }

    override fun equals(other: Any?): Boolean = other is ProductId && value == other.value

    override fun hashCode(): Int = value.hashCode()

    override fun toString(): String = value
}

private inline fun productIdResult(block: () -> ProductId): Result<ProductId> = try {
    Result.success(block())
} catch (error: ProductIdException) {
    Result.failure(error)
}
