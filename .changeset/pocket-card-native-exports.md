---
"@parity/truapi-host": patch
---

Native hosts read and screen a Pocket card's fields through the core rather than reimplementing them. `parse_renderer_node_json` reads a product-declared face from the JSON shape the generated client describes, bounded at the same nesting the renderer subscription carries, and `encode_renderer_node` / `decode_renderer_node` keep one under the SCALE encoding it already travels in, so a face kept at one version reads back at the next. `screen_pocket_card_id` and `screen_pocket_card_title` apply the rules every Pocket call already applies, so a card is refused where it is declared rather than at its first wire call, and a title keeps the display rules rather than the stricter identifier ones. Both raise `NativeRendererError` or `NativeChatFieldError`.
