//! Unified [`Renderer`] trait.

use crate::{
	CallContext, CallError, Subscription,
	versioned::renderer::{
		HostRendererActionSubscribeError, HostRendererActionSubscribeItem,
		HostRendererActionSubscribeRequest, HostRendererFailureSubscribeError,
		HostRendererFailureSubscribeItem, HostRendererFailureSubscribeRequest,
		ProductRendererRenderError, ProductRendererRenderItem, ProductRendererRenderRequest,
	},
	wire, wire_trait,
};

/// Product-rendered bodies, the actions triggered inside them, and the
/// materials that fell back in them.
#[wire_trait(id = 17)]
#[crate::service(required_execution = Worker)]
#[async_trait::async_trait]
pub trait Renderer: Send + Sync {
	/// Streams renderer trees for one product-rendered body. Each item
	/// replaces the previous tree. The stream stays open while the body is
	/// displayed so the product can redraw in place. The context may name a
	/// card the product docked in a room footer.
	///
	/// ```ts
	/// truapi.renderer.onRender(({ context, payload }, send) => {
	///   send({ tag: "String", value: { text: `${context.tag}: ${payload}` } });
	/// });
	/// ```
	#[wire(host_initiated, id = 0)]
	fn render(
		&self,
		_cx: &CallContext,
		_request: ProductRendererRenderRequest,
	) -> Subscription<ProductRendererRenderItem, CallError<ProductRendererRenderError>> {
		Subscription::interrupted(CallError::unavailable())
	}

	/// Subscribe to actions triggered inside this product's rendered bodies.
	///
	/// ```ts
	/// import { firstValueFrom, from } from "rxjs";
	///
	/// const action = await firstValueFrom(
	///   from(truapi.renderer.actionSubscribe()),
	/// );
	/// console.log("action received:", action.context, action.actionId);
	/// ```
	#[wire(id = 1)]
	async fn action_subscribe(
		&self,
		_cx: &CallContext,
		_request: HostRendererActionSubscribeRequest,
	) -> Subscription<HostRendererActionSubscribeItem, CallError<HostRendererActionSubscribeError>>
	{
		Subscription::interrupted(CallError::unavailable())
	}

	/// Subscribe to the `Materials` nodes that fell back in this product's
	/// rendered bodies, each reported once per tree.
	///
	/// ```ts
	/// import { firstValueFrom, from } from "rxjs";
	///
	/// const failure = await firstValueFrom(
	///   from(truapi.renderer.failureSubscribe()),
	/// );
	/// console.log("material fell back:", failure.context, failure.node, failure.reason);
	/// ```
	#[wire(id = 2)]
	async fn failure_subscribe(
		&self,
		_cx: &CallContext,
		_request: HostRendererFailureSubscribeRequest,
	) -> Subscription<HostRendererFailureSubscribeItem, CallError<HostRendererFailureSubscribeError>>
	{
		Subscription::interrupted(CallError::unavailable())
	}
}
