package io.paritytech.polkadotapp.feature_products_impl.data.repository

import io.paritytech.polkadotapp.common.utils.enumValueOfOrNull
import io.paritytech.polkadotapp.database.model.ProductLocal
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductIcon
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.tools_ipfs_api.Cid

internal fun ProductLocal.toProduct(): Product {
    return Product(
        id = ProductId.fromStoredValue(id),
        name = name,
        icon = toIcon(),
    )
}

private fun ProductLocal.toIcon(): ProductIcon? {
    val cid = iconCid?.let { runCatching { Cid.decode(it) }.getOrNull() } ?: return null
    val format = iconFormat?.let { enumValueOfOrNull<ProductIcon.Format>(it) } ?: return null

    return ProductIcon(cid = cid, format = format)
}

internal fun Product.toLocal(): ProductLocal {
    return ProductLocal(
        id = id.value,
        name = name,
        iconCid = icon?.cid?.toString(),
        iconFormat = icon?.format?.name,
        // Ignored by ProductDao.upsertResolved, which never writes this column.
        userWorkerUrl = null,
    )
}
