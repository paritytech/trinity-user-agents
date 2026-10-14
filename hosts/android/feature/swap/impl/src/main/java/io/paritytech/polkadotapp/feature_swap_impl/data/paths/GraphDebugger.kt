package io.paritytech.polkadotapp.feature_swap_impl.data.paths

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.assetOrNull
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.chains.multiNetwork.getChainOrNull
import io.paritytech.polkadotapp.common.utils.graph.Edge
import io.paritytech.polkadotapp.common.utils.graph.Graph
import io.paritytech.polkadotapp.common.utils.graph.allEdges
import io.paritytech.polkadotapp.common.utils.graph.vertices
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapGraphEdge
import timber.log.Timber
import javax.inject.Inject

class GraphDebugger @Inject constructor(
    private val chainRegistry: ChainRegistry,
) {
    suspend fun logGraphDiagram(graph: Graph<FullChainAssetId, SwapGraphEdge>) {
        val content = graph.toMermaidGrouped(
            nodeId = { sanitizeId("${it.chainId}:${it.assetId}") },
            nodeLabel = {
                chainRegistry.assetOrNull(it)?.let { asset ->
                    asset.symbol
                }
            },
            edgeLabel = { it.debugLabel() },
            groupBy = { it.chainId },
            groupTitle = { chainRegistry.getChainOrNull(it)?.name ?: it }
        )

        Timber.d("=========== Diagram ===========")
        Timber.d(content)
        Timber.d("=========== Diagram ===========\n\n\n")
    }

    /**
     * This is heavily vibe-coded don't pay too much attention to this code
     */
    suspend fun <N, E : Edge<N>> Graph<N, E>.toMermaidGrouped(
        direction: MermaidDirection = MermaidDirection.LR,
        groupBy: suspend (N) -> String?, // e.g. { it.chainId }
        groupTitle: suspend (String) -> String = { "Chain: $it" },
        nodeLabel: suspend (N) -> String? = { it.toString() },
        nodeId: (N) -> String = { sanitizeId(it.toString()) },
        edgeLabel: suspend (E) -> String? = { "" },
        directed: Boolean = true
    ): String {
        val sb = StringBuilder()

        // Collect nodes (sources + targets)
        val nodes = linkedSetOf<N>().apply {
            addAll(vertices())
            allEdges().forEach { add(it.to) }
        }

        // Resolve node label/id/group (skip nodes with null label)
        data class NodeInfo(val id: String, val label: String, val group: String?)
        val usedIds = mutableSetOf<String>()
        val nodeInfo = mutableMapOf<N, NodeInfo>()
        for (n in nodes) {
            val lbl = nodeLabel(n) ?: continue
            val grp = groupBy(n)
            val base = nodeId(n).ifBlank { "n_${n.hashCode()}" }
            var id = base; var i = 1
            while (!usedIds.add(id)) { id = "${base}_$i"; i++ }
            nodeInfo[n] = NodeInfo(id, lbl, grp)
        }

        // Partition nodes by group
        val byGroup: Map<String?, List<N>> = nodeInfo.keys.groupBy({ nodeInfo[it]!!.group })

        // Helper to print an edge
        fun StringBuilder.emitEdge(fromId: String, toId: String, label: String, directed: Boolean) {
            val clean = escapeMermaidText(label)
            if (directed) {
                if (clean.isEmpty()) appendLine("  $fromId --> $toId")
                else appendLine("  $fromId --$clean--> $toId")
            } else {
                if (clean.isEmpty()) appendLine("  $fromId --- $toId")
                else appendLine("  $fromId ---$clean $toId")
            }
        }

        // Start diagram
        sb.appendLine("flowchart ${direction.token}")

        // 1) Emit grouped subgraphs (nodes + intra-group edges)
        for ((gKey, members) in byGroup) {
            if (gKey == null) continue
            val title = escapeMermaidLabel(groupTitle(gKey))
            val subgraphId = "cluster_${sanitizeId(gKey)}"
            sb.appendLine("  subgraph $subgraphId[\"$title\"]")
            sb.appendLine("    direction TB") // internal layout inside the cluster

            // Node declarations
            for (n in members) {
                val info = nodeInfo[n] ?: continue
                sb.appendLine("    ${info.id}[\"${escapeMermaidLabel(info.label)}\"]")
            }

            // Intra-group edges: both endpoints in 'members'
            val memberSet = members.toHashSet()
            adjacencyList.forEach { (from, edges) ->
                if (from !in memberSet) return@forEach
                val fromInfo = nodeInfo[from] ?: return@forEach
                for (e in edges) {
                    val toInfo = nodeInfo[e.to] ?: continue
                    if (e.to !in memberSet) continue
                    val lbl = edgeLabel(e) ?: continue
                    // Note: we’re inside subgraph—indent one level
                    val edgeLine = buildString {
                        emitEdge(fromInfo.id, toInfo.id, lbl, directed)
                    }.trimStart()
                    sb.append("    ").append(edgeLine)
                }
            }

            sb.appendLine("  end")
        }

        // 2) Emit ungrouped nodes
        byGroup[null]?.forEach { n ->
            val info = nodeInfo[n] ?: return@forEach
            sb.appendLine("  ${info.id}[\"${escapeMermaidLabel(info.label)}\"]")
        }

        // 3) Inter-group edges (or grouped <-> ungrouped)
        adjacencyList.forEach { (from, edges) ->
            val fromInfo = nodeInfo[from] ?: return@forEach
            for (e in edges) {
                val toInfo = nodeInfo[e.to] ?: continue
                // If both ends share the same non-null group, it was already printed inside the subgraph.
                val sameGroup = fromInfo.group != null && fromInfo.group == toInfo.group
                if (sameGroup) continue
                val lbl = edgeLabel(e) ?: continue
                sb.emitEdge(fromInfo.id, toInfo.id, lbl, directed)
            }
        }

        return sb.toString()
    }

    // ---- helpers ----

    enum class MermaidDirection(val token: String) { LR("LR"), RL("RL"), TB("TB"), BT("BT") }

    private fun escapeMermaidLabel(s: String): String =
        s.replace("\"", "&quot;").replace("]", "&#93;").replace("\n", "<br/>")

    private fun escapeMermaidText(s: String): String =
        s.replace("|", "\\|").replace("\n", "<br/>")

    private fun sanitizeId(s: String): String =
        buildString(s.length) {
            for (ch in s) append(
                when {
                    ch.isLetterOrDigit() -> ch
                    ch == '_' -> '_'
                    else -> '_'
                }
            )
        }.ifEmpty { "n_${s.hashCode()}" }
}
