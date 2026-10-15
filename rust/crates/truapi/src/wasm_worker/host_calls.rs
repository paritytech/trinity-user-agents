//! Calls the host starts into a wasm worker: the host-initiated methods, such
//! as `renderer_render` and `chat_user_action`.

use core::{
	pin::Pin,
	task::{Context, Poll},
};
use std::sync::{
	Arc,
	atomic::{AtomicU32, Ordering},
};

use futures::{Stream, StreamExt, channel::mpsc};
use parity_scale_codec::Encode;

use crate::CallError;

/// One answer of a worker to a call the host started. The payloads are the
/// versioned SCALE values of the frame path, in the version of the request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostCallEvent {
	/// One item.
	Item(Vec<u8>),
	/// The end: a SCALE `Result<(), CallError<Error>>`. No event follows it.
	End(Vec<u8>),
}

impl HostCallEvent {
	/// The end of a call that the worker cannot answer.
	pub fn failure(error: CallError<()>) -> Self {
		Self::End(Result::<(), CallError<()>>::Err(error).encode())
	}
}

/// Where the worker sends the answers of one call.
pub type Answers = Box<dyn FnMut(HostCallEvent) + Send>;

/// What a [`HostCalls`] handle asks of the worker.
pub enum HostCommand {
	/// Start a call of `method` with an encoded versioned request.
	Start {
		/// The id of the call, unique for the worker.
		call: u32,
		/// The wire name of the method.
		method: String,
		/// The SCALE versioned request.
		request: Vec<u8>,
		/// Where the worker sends the answers.
		answers: Answers,
	},
	/// Stop a call.
	Cancel {
		/// The id of the call.
		call: u32,
	},
}

/// A handle that starts calls into one wasm worker, from any thread. The
/// worker takes the calls while it runs. When no handle is left, no call can
/// come, so a worker that waits on nothing else stalls.
#[derive(Clone)]
pub struct HostCalls {
	commands: mpsc::UnboundedSender<HostCommand>,
	next_call: Arc<AtomicU32>,
}

/// The handle of a worker and the receiving side the worker takes calls from.
pub fn channel() -> (HostCalls, mpsc::UnboundedReceiver<HostCommand>) {
	let (commands, inbox) = mpsc::unbounded();
	(HostCalls { commands, next_call: Arc::new(AtomicU32::new(0)) }, inbox)
}

impl HostCalls {
	/// Start a call of `method`, the wire name of a host-initiated method,
	/// such as `chat_user_action`, with an encoded versioned request. The
	/// worker gives each answer to `answers` on the task that runs it, and the
	/// last answer is an [`HostCallEvent::End`]. A worker that ended, or has no
	/// handler for `method`, ends the call at once. Dropping the returned
	/// [`HostCall`] before the end stops the call in the worker.
	pub fn start(
		&self,
		method: &str,
		request: Vec<u8>,
		answers: impl FnMut(HostCallEvent) + Send + 'static,
	) -> HostCall {
		let call = self.next_call.fetch_add(1, Ordering::Relaxed);
		let command = HostCommand::Start {
			call,
			method: method.to_string(),
			request,
			answers: Box::new(answers),
		};
		if let Err(refused) = self.commands.unbounded_send(command) &&
			let HostCommand::Start { mut answers, .. } = refused.into_inner()
		{
			answers(worker_ended());
		}
		HostCall { call, commands: self.commands.clone() }
	}

	/// Start a call as [`Self::start`] does, and take its answers as a
	/// stream. Dropping the stream before the end stops the call.
	pub fn stream(&self, method: &str, request: Vec<u8>) -> HostCallStream {
		let (sender, events) = mpsc::unbounded();
		let call = self.start(method, request, move |event| {
			let _ = sender.unbounded_send(event);
		});
		HostCallStream { _call: call, events }
	}
}

/// The end of a call that a worker which no longer runs cannot answer.
pub fn worker_ended() -> HostCallEvent {
	HostCallEvent::failure(CallError::HostFailure { reason: "the wasm worker ended".to_string() })
}

/// One call the host started into a worker. Dropping it stops the call,
/// unless the call ended before.
pub struct HostCall {
	call: u32,
	commands: mpsc::UnboundedSender<HostCommand>,
}

impl Drop for HostCall {
	fn drop(&mut self) {
		let _ = self.commands.unbounded_send(HostCommand::Cancel { call: self.call });
	}
}

/// The answers of one call, from [`HostCalls::stream`]. It yields items,
/// then one end, then nothing.
pub struct HostCallStream {
	_call: HostCall,
	events: mpsc::UnboundedReceiver<HostCallEvent>,
}

impl Stream for HostCallStream {
	type Item = HostCallEvent;

	fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		self.events.poll_next_unpin(context)
	}
}
