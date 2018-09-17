/* Copyright (c) 2018 University of Utah
 *
 * Permission to use, copy, modify, and distribute this software for any
 * purpose with or without fee is hereby granted, provided that the above
 * copyright notice and this permission notice appear in all copies.
 *
 * THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR(S) DISCLAIM ALL WARRANTIES
 * WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
 * MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL AUTHORS BE LIABLE FOR
 * ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
 * WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
 * ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
 * OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
 */

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering};

use super::cycles;
use super::rpc;
use super::task::Task;
use super::task::TaskState::*;

use e2d2::common::EmptyMetadata;
use e2d2::headers::IpHeader;
use e2d2::interface::Packet;

use spin::RwLock;
use std::sync::Arc;
use task::TaskPriority;

/// A simple round robin scheduler for Tasks in Sandstorm.
pub struct RoundRobin {
    // The time-stamp at which the scheduler last ran. Required to identify whether there is an
    // uncooperative task running on the scheduler.
    latest: AtomicUsize,

    // Atomic flag indicating whether there is a malicious/long running procedure on this
    // scheduler. If true, the scheduler must return down to Netbricks on the next call to poll().
    compromised: AtomicBool,

    // Identifier of the thread this scheduler is running on. Required for pre-emption.
    thread: AtomicUsize,

    // Identifier of the core this scheduler is running on. Required for pre-emption.
    core: AtomicIsize,

    // Run-queue of tasks waiting to execute. Tasks on this queue have either yielded, or have been
    // recently enqueued and never run before.
    waiting: RwLock<VecDeque<Box<Task>>>,

    // Response packets returned by completed tasks. Will be picked up and sent out the network by
    // the Dispatch task.
    responses: RwLock<Vec<Packet<IpHeader, EmptyMetadata>>>,

    // Sibling schedulers to steal from the tasks that are already constructed.
    sibling_scheds: Arc<RwLock<Vec<Arc<RoundRobin>>>>,
}

// Implementation of methods on RoundRobin.
impl RoundRobin {
    /// Creates and returns a round-robin scheduler that can run tasks implementing the `Task`
    /// trait.
    ///
    /// # Arguments
    ///
    /// * `thread`: Identifier of the thread this scheduler will run on.
    /// * `core`:   Identifier of the core this scheduler will run on.
    pub fn new(thread: u64, core: i32) -> RoundRobin {
        RoundRobin {
            latest: AtomicUsize::new(cycles::rdtsc() as usize),
            compromised: AtomicBool::new(false),
            thread: AtomicUsize::new(thread as usize),
            core: AtomicIsize::new(core as isize),
            waiting: RwLock::new(VecDeque::new()),
            responses: RwLock::new(Vec::new()),
            sibling_scheds: Arc::new(RwLock::new(Vec::with_capacity(8))),
        }
    }

    /// Enqueues a task onto the scheduler. The task is enqueued at the end of the schedulers
    /// queue.
    ///
    /// # Arguments
    ///
    /// * `task`: The task to be added to the scheduler. Must implement the `Task` trait.
    #[inline]
    pub fn enqueue(&self, task: Box<Task>) {
        self.waiting.write().push_back(task);
    }

    /// Enqueues multiple tasks onto the scheduler.
    ///
    /// # Arguments
    ///
    /// * `tasks`: A deque of tasks to be added to the scheduler. These tasks will be run in the
    ///            order that they are provided in, and must implement the `Task` trait.
    #[inline]
    pub fn enqueue_many(&self, mut tasks: VecDeque<Box<Task>>) {
        self.waiting.write().append(&mut tasks);
    }

    /// Dequeues all waiting tasks from the scheduler.
    ///
    /// # Return
    ///
    /// A deque of all waiting tasks in the scheduler. This tasks might be in various stages of
    /// execution. Some might have run for a while and yielded, and some might have never run
    /// before. If there are no tasks waiting to run, then an empty vector is returned.
    #[inline]
    pub fn dequeue_all(&self) -> VecDeque<Box<Task>> {
        let mut tasks = self.waiting.write();
        return tasks.drain(..).collect();
    }

    /// Returns a list of pending response packets.
    ///
    /// # Return
    ///
    /// A vector of response packets that were returned by tasks that completed execution. This
    /// packets should be sent out the network. If there are no pending responses, then an empty
    /// vector is returned.
    #[inline]
    pub fn responses(&self) -> Vec<Packet<IpHeader, EmptyMetadata>> {
        let mut responses = self.responses.write();
        return responses.drain(..).collect();
    }

    /// Appends a list of responses to the scheduler.
    ///
    /// # Arguments
    ///
    /// * `resps`: A vector of response packets parsed upto their IP headers.
    pub fn append_resps(&self, resps: &mut Vec<Packet<IpHeader, EmptyMetadata>>) {
        self.responses.write().append(resps);
    }

    /// Adds sibling schedulers
    pub fn add_siblings(&self, sibs: &mut Arc<RwLock<Vec<Arc<RoundRobin>>>>) {
        self.sibling_scheds.write().append(&mut sibs.write());
    }

    /// Returns the time-stamp at which the latest scheduling decision was made.
    #[inline]
    pub fn latest(&self) -> u64 {
        self.latest.load(Ordering::Relaxed) as u64
    }

    /// Sets the compromised flag on the scheduler.
    #[inline]
    pub fn compromised(&self) {
        self.compromised.store(true, Ordering::Relaxed);
    }

    /// Returns the identifier of the thread this scheduler was configured to run on.
    #[inline]
    pub fn thread(&self) -> u64 {
        self.thread.load(Ordering::Relaxed) as u64
    }

    /// Returns the identifier of the core this scheduler was configured to run on.
    #[inline]
    pub fn core(&self) -> i32 {
        self.core.load(Ordering::Relaxed) as i32
    }

    /// Picks up a task from the waiting queue, and runs it until it either yields or completes.
    pub fn poll(&self) {
        // Sibling id indexed into sibling_scheds Vec to select sibling to steal
        // work from. It is incremented after each selection so we move further right when stealing
        // work in later iterations.
        let mut sibling_id: usize = 0;

        // Lets scheduler take turns between its own NIC queue and sibling's tasks queue.
        let mut sibling_turn: bool = false;

        loop {
            // Set the time-stamp of the latest scheduling decision.
            self.latest
                .store(cycles::rdtsc() as usize, Ordering::Relaxed);

            // If the compromised flag was set, then return.
            if self.compromised.load(Ordering::Relaxed) {
                return;
            }

            // If there are tasks to run, then pick one from the head of the queue, and run it until it
            // either completes or yields back.
            let task = self.waiting.write().pop_front();

            if let Some(mut task) = task {
                if task.run().0 == COMPLETED {
                    // The task finished execution, check for request and response packets. If they
                    // exist, then free the request packet, and enqueue the response packet.
                    if let Some((req, res)) = unsafe { task.tear() } {
                        req.free_packet();
                        self.responses
                            .write()
                            .push(rpc::fixup_header_length_fields(res));
                    }
                } else {
                    // The task did not complete execution. Add it back to the waiting list so that it
                    // gets to run again.
                    self.waiting.write().push_back(task);
                }
            }

            if self.waiting.write().len() == 1 {
                // Tried its own NIC queue, then sibling's NIC queue
                // but found no work.

                if sibling_turn {
                    // Swith flag so next time sched polls its own NIC queue.
                    sibling_turn = false;

                    let mut task: Option<Box<Task>> = None;

                    // Assuming we have 8 schedulers.
                    sibling_id = (sibling_id + 1) % 7;

                    // Try stealing from sibling's tasks queue
                    if let Some(sibs) = self.sibling_scheds.try_write() {
                        if let Some(mut sib_waiting_queue) = sibs[sibling_id].waiting.try_write() {
                            if sib_waiting_queue.len() > 1 {
                                // There are tasks to steal.
                                //task = sib_waiting_queue.front().as_ref().unwrap();

                                //if task.priority() != TaskPriority::DISPATCH {
                                if sib_waiting_queue.front().unwrap().priority()
                                    != TaskPriority::DISPATCH
                                {
                                    task = sib_waiting_queue.pop_front();
                                } else {
                                    task = sib_waiting_queue.pop_back();
                                }
                            }
                        }
                    }

                    if task.is_some() {
                        if let Some(mut task) = task {
                            if task.run().0 == COMPLETED {
                                // The task finished execution, check for request and response packets. If they
                                // exist, then free the request packet, and enqueue the response packet.
                                if let Some((req, res)) = unsafe { task.tear() } {
                                    req.free_packet();
                                    self.responses
                                        .write()
                                        .push(rpc::fixup_header_length_fields(res));
                                }
                            } else {
                                // The task did not complete execution. Add it back to the waiting list so that it
                                // gets to run again.
                                self.waiting.write().push_back(task);
                            }
                        }
                    }
                } else {
                    sibling_turn = true;
                }
            }
        }
    }
}

// RoundRobin uses atomics and RwLocks. Hence, it is thread-safe. Need to explicitly mark it as
// Send and Sync here because the compiler does not do so. This is because Packet contains a *mut
// MBuf which is not Send and Sync. Similarly, the compiler appears to be having trouble with the
// "Task" trait object.
unsafe impl Send for RoundRobin {}
unsafe impl Sync for RoundRobin {}
