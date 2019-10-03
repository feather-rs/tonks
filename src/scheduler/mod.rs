use crate::scheduler::stage::Stage;
use crate::EventedId;
use crossbeam::{Receiver, Sender};
use hashbrown::HashMap;
use shred::{RunNow, World};

mod run_now;
mod stage;

/// Internal ID used to identify resources.
/// These are allocated consecutively so that
/// they can be used as indices into vectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct InternalResourceId(usize);
/// Internal ID used to identify systems (and event handlers).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct SystemId(usize);
/// Internal ID used to identify event types.
/// These are allocated consecutively so that
/// they can be used as indices into vectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct InternalEventedId(usize);

/// Message sent from an arbitrary system to the scheduler.
/// The system ID must be sent as well, since the channel is shared
/// across all systems.
#[derive(Debug)]
struct SystemToSchedulerMsg {
    /// ID of the system sending this message
    id: SystemId,
    /// Message payload
    payload: SystemToSchedulerPayload,
}

#[derive(Debug)]
enum SystemToSchedulerPayload {
    /// The system completed execution.
    Finished,
}

/// Type of a system.
#[derive(Debug, Clone, Copy)]
enum SystemType {
    System,
    EventHandler,
}

pub struct Scheduler {
    /// Vector of counters representing the current number of event
    /// handlers which must use any given resource before a system
    /// can access it.
    ///
    /// Doing this allows for our core guarantee: that when an event
    /// marked with `BeforeAccesses` is triggered, any resources to which
    /// its event handlers __write__ will not be read from until the handler
    /// completes.
    ///
    /// Why `u8`? Cache locality. If someone needs more than what can fit
    /// into a `u8`, that's not our problem.
    ///
    /// This is indexed by the `InternalResourceId`.
    event_handler_uses: Vec<u8>,
    /// Channel used to communicate with currently running systems and event handlers.
    receiver: Receiver<SystemToSchedulerMsg>,
    /// Sender which is attached to `receiver` and is cloned to send to systems.
    sender: Sender<SystemToSchedulerMsg>,
    /// Vector of systems and event handlers that can be run.
    ///
    /// This is indexed by the `SystemId`.
    systems: Vec<Box<dyn for<'a> RunNow<'a>>>,
    /// Vector of types for each system.
    ///
    /// This is indexed by the `SystemId`.
    system_types: Vec<SystemType>,
    /// Stages of normal system execution.
    stages: Vec<Stage>,
    /// Vector of event handler pipelines. Each pipeline
    /// is effectively another vector of stages.
    ///
    /// This vector is indexed by the `InternalEventedId`.
    event_pipelines: Vec<Vec<Stage>>,
    /// Mapping from `EventedId` to `InternalEventedId`.
    evented_id_map: HashMap<EventedId, InternalEventedId>,
}

impl Scheduler {
    /// Runs all systems in the scheduler and handles
    /// any triggered events.
    ///
    /// Before this call returns, any newly triggered events
    /// are guaranteed to have been executed according to their
    /// `EventExecutionStrategy`.
    pub fn run(&mut self, world: &World) {
        unimplemented!()
    }
}
