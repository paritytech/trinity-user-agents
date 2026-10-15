//! What the exports [`main`](crate::main) emits run: an executor for the
//! entry point and the tasks of [`spawn`](crate::spawn).
//!
//! Each task has its own waker. The executor polls a task only when its
//! waker is woken: at start, after a host event for one of its calls, or
//! after a different task wakes it. One entry from the host polls until no task
//! is woken. When the entry point returns, the executor reports the outcome
//! to the host and polls nothing again.

use core::{
	cell::{Cell, RefCell},
	future::Future,
	pin::Pin,
	sync::atomic::{AtomicBool, Ordering},
	task::{Context, Waker},
};
use std::{
	collections::{HashMap, VecDeque},
	sync::{Arc, Mutex},
	task::Wake,
};

use truapi::{guest::deliver, wasm_abi::EventKind};

use crate::{Error, imports};

type Task = Pin<Box<dyn Future<Output = ()>>>;

/// Identifier of the entry point task.
const ENTRY: u64 = 0;

thread_local! {
	/// The tasks that did not return. A task is out of the map while it polls.
	static TASKS: RefCell<HashMap<u64, Task>> = RefCell::new(HashMap::new());
	/// The waker of each task, made once for the task.
	static WAKERS: RefCell<HashMap<u64, Arc<TaskWaker>>> = RefCell::new(HashMap::new());
	static NEXT_TASK: Cell<u64> = const { Cell::new(ENTRY + 1) };
	/// The entry point returned, so the worker ended.
	static ENDED: Cell<bool> = const { Cell::new(false) };
}

/// Tasks woken and not yet polled, in the order of their wakes.
static WOKEN: Mutex<VecDeque<u64>> = Mutex::new(VecDeque::new());

struct TaskWaker {
	task: u64,
	/// The task is in [`WOKEN`]. A second wake does not add it again.
	queued: AtomicBool,
}

impl Wake for TaskWaker {
	fn wake(self: Arc<Self>) {
		self.wake_by_ref();
	}

	fn wake_by_ref(self: &Arc<Self>) {
		if !self.queued.swap(true, Ordering::AcqRel) {
			WOKEN
				.lock()
				.unwrap_or_else(|poisoned| poisoned.into_inner())
				.push_back(self.task);
		}
	}
}

fn insert(task: u64, future: Task) {
	let waker = Arc::new(TaskWaker { task, queued: AtomicBool::new(false) });
	TASKS.with_borrow_mut(|tasks| tasks.insert(task, future));
	WAKERS.with_borrow_mut(|wakers| wakers.insert(task, waker.clone()));
	waker.wake();
}

/// Add `future` as a new task. The executor polls it first after the task
/// that spawned it yields.
pub fn spawn(future: impl Future<Output = ()> + 'static) {
	if ENDED.get() {
		return;
	}
	let task = NEXT_TASK.get();
	NEXT_TASK.set(task + 1);
	insert(task, Box::pin(future));
}

/// Run `entry` until no task is woken, reporting its outcome to the host
/// when it returns.
pub fn start(entry: impl Future<Output = Result<(), Error>> + 'static) {
	let entry = async move {
		match entry.await {
			Ok(()) => imports::finish(1, core::ptr::null(), 0),
			Err(Error(message)) => imports::finish(0, message.as_ptr(), message.len() as u32),
		}
	};
	insert(ENTRY, Box::pin(entry));
	run_woken();
}

/// Read one host event of `len` bytes, hand it to the call it belongs to,
/// then poll the tasks that it wakes.
pub fn on_event(handle: u32, kind: u32, len: u32) {
	let kind = EventKind::try_from(kind).expect("host sent an unknown event kind");
	let mut payload = vec![0u8; len as usize];
	imports::read_event(payload.as_mut_ptr());
	if ENDED.get() {
		return;
	}
	deliver(handle, kind, payload);
	run_woken();
}

fn next_woken() -> Option<u64> {
	WOKEN.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).pop_front()
}

fn run_woken() {
	while let Some(task) = next_woken() {
		if ENDED.get() {
			return;
		}
		let Some(waker) = WAKERS.with_borrow(|wakers| wakers.get(&task).cloned()) else {
			continue;
		};
		let Some(mut future) = TASKS.with_borrow_mut(|tasks| tasks.remove(&task)) else {
			continue;
		};
		// A wake during the poll queues the task again.
		waker.queued.store(false, Ordering::Release);
		let waker = Waker::from(waker);
		if future.as_mut().poll(&mut Context::from_waker(&waker)).is_ready() {
			WAKERS.with_borrow_mut(|wakers| wakers.remove(&task));
			if task == ENTRY {
				end();
				return;
			}
		} else {
			TASKS.with_borrow_mut(|tasks| tasks.insert(task, future));
		}
	}
}

/// Drop every task, which releases their calls, and stop polling.
fn end() {
	ENDED.set(true);
	let tasks = TASKS.with_borrow_mut(core::mem::take);
	drop(tasks);
	WAKERS.with_borrow_mut(HashMap::clear);
	WOKEN.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clear();
}
