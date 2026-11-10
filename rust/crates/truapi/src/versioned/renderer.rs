//! Versioned wrappers for [`Renderer`](crate::api::Renderer) methods.

use crate::v01;

truapi_macros::versioned_type! {
	pub enum ProductRendererRenderRequest { V1 => v01::ProductRendererRenderRequest }
	pub enum ProductRendererRenderItem { V1 => v01::RendererNode }
	pub enum ProductRendererRenderError { V1 => v01::GenericError }
	pub enum HostRendererActionSubscribeRequest { V1 }
	pub enum HostRendererActionSubscribeItem { V1 => v01::HostRendererActionSubscribeItem }
	pub enum HostRendererActionSubscribeError { V1 => v01::GenericError }
	pub enum HostRendererFailureSubscribeRequest { V1 }
	pub enum HostRendererFailureSubscribeItem { V1 => v01::HostRendererFailureSubscribeItem }
	pub enum HostRendererFailureSubscribeError { V1 => v01::GenericError }
}

#[cfg(test)]
mod tests;
