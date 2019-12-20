use bit_set::BitSet;
use bumpalo::Bump;
use crossbeam::{Receiver, Sender};
use rayon::prelude::*;
use smallvec::{smallvec, SmallVec};
use std::collections::VecDeque;
use thread_local::ThreadLocal;

mod builder;

use crate::event::event_id_for;
use crate::system::SystemCtx;
use crate::{
    resources::RESOURCE_ID_MAPPINGS, system::SYSTEM_ID_MAPPINGS, Event, EventId, RawEventHandler,
    RawSystem, ResourceId, Resources, SystemId,
};
pub use builder::{EventsBuilder, SchedulerBuilder};
use legion::world::World;
use std::iter;
use std::sync::Arc;

/// Context of a running system, used for internal purposes.
#[derive(Clone)]
pub struct Context {
    /// The ID of this system.
    pub(crate) id: SystemId,
    /// Sender for communicating with the scheduler.
    pub(crate) sender: Sender<TaskMessage>,
}

/// ID of a stage, allocated consecutively for use as indices into vectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct StageId(usize);

/// A stage in the completion of a dispatch. Each stage
/// contains systems which can be executed in parallel.
type Stage = SmallVec<[SystemId; 6]>;

type ResourceVec = SmallVec<[ResourceId; 8]>;

/// A raw pointer to some `T`.
///
/// # Safety
/// This type implements `Send` and `Sync`, but it is
/// up to the user to ensure that:
/// * If the pointer is dereferenced, the value has not been dropped.
/// * No data races occur.
struct SharedRawPtr<T: ?Sized>(*const T);

unsafe impl<T: ?Sized + Send> Send for SharedRawPtr<T> {}
unsafe impl<T: ?Sized + Sync> Sync for SharedRawPtr<T> {}

type DynSystem = (dyn RawSystem + 'static);

/// A mutable raw pointer to some `T`.
///
/// # Safety
/// This type implements `Send` and `Sync`, but it is
/// up to the user to ensure that:
/// * If the pointer is dereferenced, the value has not been dropped.
/// * No data races occur.
struct SharedMutRawPtr<T: ?Sized>(*mut T);

unsafe impl<T: ?Sized + Send> Send for SharedMutRawPtr<T> {}
unsafe impl<T: ?Sized + Sync> Sync for SharedMutRawPtr<T> {}

/// A message sent from a running task to the scheduler.
#[allow(dead_code)]
pub(crate) enum TaskMessage {
    /// Indicates that the system with the given ID
    /// completed.
    ///
    /// Note that this is not sent for ordinary systems in
    /// a stage, since the overhead of sending so many
    /// messages would be too great. Instead, `StageComplete`
    /// is sent to indicate that all systems in a stage completed
    /// at once.
    ///
    /// This is only used for oneshot systems.
    SystemComplete(SystemId),
    /// Indicates that all systems in a stage have completed.
    StageComplete(StageId),
    /// Indicates that an event handler pipeline has finished running.
    EventHandlingComplete(EventId),
    /// Requests that one or more events be handled.
    ///
    /// `EndOfSystem` handlers should be run after this message is sent.
    ///
    /// # Safety
    /// * The event ID must correspond to the type of event being handled.
    /// * `ptr` must be a pointer to an array of events of the corresponding
    /// type, allocated in one of the thread-local bump allocators.
    TriggerEvents {
        id: EventId,
        ptr: *const (),
        len: usize,
    },
}

unsafe impl Send for TaskMessage {}
unsafe impl Sync for TaskMessage {}

/// A task to run. This can either be a stage (mutliple systems run in parallel),
/// a oneshot system, or an event handling pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)]
enum Task {
    Stage(StageId),
    Oneshot(SystemId),
    HandleEvent(EventId, *const (), usize),
    // TODO: event pipeline
}

// Safety: *const [()] is allocated in the bump allocator,
// which is not reset until execute() returns (and thus all tasks
// have been consumed.)
unsafe impl Send for Task {}
unsafe impl Sync for Task {}

pub trait OrExtend<T> {
    fn set_or_extend(&mut self, index: usize, value: T);
    fn get_mut_or_extend(&mut self, index: usize) -> &mut T;
}

impl<T: Default> OrExtend<T> for Vec<T> {
    fn set_or_extend(&mut self, index: usize, value: T) {
        if index >= self.len() {
            self.extend(iter::repeat_with(|| T::default()).take(index - self.len() + 1));
        }
        self[index] = value;
    }

    fn get_mut_or_extend(&mut self, index: usize) -> &mut T {
        if index >= self.len() {
            self.extend(iter::repeat_with(|| T::default()).take(index - self.len() + 1));
        }
        &mut self[index]
    }
}

/// The `tonks` scheduler. This is similar to `shred::Dispatcher`
/// but has more features.
#[derive(Derivative)]
#[derivative(Debug)]
pub struct Scheduler {
    /// Resources held by this scheduler.
    #[derivative(Debug = "ignore")]
    resources: Resources,

    /// Queue of tasks to run during the current dispatch.
    ///
    /// Each dispatch, this queue is reset to `starting_queue`.
    task_queue: VecDeque<Task>,
    /// The starting task queue for any dispatch.
    starting_queue: VecDeque<Task>,

    /// Bit set representing write resources which are currently held.
    ///
    /// This set is indexed by the `ResourceId`.
    writes_held: BitSet,
    /// Vector of reference counts representing the number of tasks currently
    /// holding a shared reference to a resource.
    ///
    /// This vector is indexed by the `ResourceId`.
    reads_held: Vec<u32>,

    /// Thread-local bump allocator used to allocate events.
    ///
    /// TODO: implement a lock-free bump arena instead.
    #[derivative(Debug = "ignore")]
    bump: Arc<ThreadLocal<Bump>>,

    /// Number of currently running systems.
    runnning_systems_count: usize,
    /// Bit set containing bits set for systems which are currently running.
    ///
    /// This is indexed by the `SystemId`.
    running_systems: BitSet,

    /// Vector of systems which can be executed. This includes oneshottable
    /// systems as well.
    ///
    /// This vector is indexed by the `SystemId`.
    #[derivative(Debug = "ignore")]
    systems: Vec<Option<Box<DynSystem>>>,
    /// Vector containing the systems for each stage.
    stages: Vec<Stage>,

    /// Vector containing the reads required for each system.
    ///
    /// This vector is indexed by the `SystemId`.
    system_reads: Vec<ResourceVec>,
    /// Vector containing the writes required for each system.
    ///
    /// This vector is indexed by the `SystemId`.
    system_writes: Vec<ResourceVec>,
    /// Vector containing the reads required for each stage.
    ///
    /// This vector is indexed by the `StageId`.
    stage_reads: Vec<ResourceVec>,
    /// Vector containing the writes required for each stage.
    ///
    /// This vector is indexed by the `StageId`.
    stage_writes: Vec<ResourceVec>,

    // === Event handling ===
    /// Vector containing event handlers. This vector is indexed by the `SystemID`.
    ///
    /// Entries which correspond to normal systems rather than event handlers
    /// are `None`.
    #[derivative(Debug = "ignore")]
    event_handlers: Vec<Option<Box<dyn RawEventHandler>>>,

    /// Vector containing the event handler system IDs required for handling
    /// each given event. All handlers in this vector have `EndOfTick` handle
    /// strategies; `EndOfSystem` is handled separately.
    ///
    /// This vector is indexed by the `EventId`.
    end_of_tick_handlers: Vec<SmallVec<[SystemId; 4]>>,

    /// Vector containing the reads required for each event handler __pipeline__.
    ///
    /// This vector is indexed by the `EventId`.
    event_reads: Vec<ResourceVec>,

    /// Vector containing the writes required for each event handler __pipeline__.
    ///
    /// This vector is indexed by the `EventId`.
    event_writes: Vec<ResourceVec>,

    /// Receiving end of the channel used to communicate with running systems.
    #[derivative(Debug = "ignore")]
    receiver: Receiver<TaskMessage>,
    /// Sending end of the above channel. This can be cloned and sent to systems.
    #[derivative(Debug = "ignore")]
    sender: Sender<TaskMessage>,

    is_first_run: bool,
}

impl Scheduler {
    /// Creates a new `Scheduler` with the given stages.
    ///
    /// `deps` is a vector indexed by the system ID containing
    /// resources for each system.
    ///
    /// # Safety
    /// The stages are assumed to have been assembled correctly:
    /// no two systems in a stage may conflict with each other.
    unsafe fn new(
        stages: Vec<Vec<Box<DynSystem>>>,
        end_of_dispatch_handlers: Vec<Vec<Box<dyn RawEventHandler>>>,
        read_deps: Vec<Vec<ResourceId>>,
        write_deps: Vec<Vec<ResourceId>>,
        resources: Resources,
    ) -> Self {
        // Detect resources used by systems and create those vectors.
        // Also collect systems into uniform vector.
        let num_systems = SYSTEM_ID_MAPPINGS.lock().len();
        let mut system_reads: Vec<ResourceVec> = iter::repeat_with(|| smallvec![])
            .take(num_systems)
            .collect();
        let mut system_writes: Vec<ResourceVec> = iter::repeat_with(|| smallvec![])
            .take(num_systems)
            .collect();
        let mut stage_reads: Vec<ResourceVec> = vec![];
        let mut stage_writes: Vec<ResourceVec> = vec![];
        let mut systems: Vec<_> = iter::repeat_with(|| None).take(num_systems).collect();
        let mut stage_systems = vec![];

        let mut counter = 0;
        for stage in stages {
            let mut stage_read = vec![];
            let mut stage_write = vec![];
            let mut systems_in_stage = smallvec![];

            for system in stage {
                let id = system.id();
                system_reads[id.0] = read_deps[counter].iter().copied().collect();
                system_writes[id.0] = write_deps[counter].iter().copied().collect();
                stage_read.extend(system_reads[counter].clone());
                stage_write.extend(system_writes[counter].clone());
                systems[id.0] = Some(system);
                systems_in_stage.push(id);
                counter += 1;
            }

            stage_reads.push(stage_read.into_iter().collect());
            stage_writes.push(stage_write.into_iter().collect());
            stage_systems.push(systems_in_stage);
        }

        // Construct event handlers
        let construct_end_of_dispatch_handlers = end_of_dispatch_handlers
            .iter()
            .map(|handlers| handlers.iter().map(|handler| handler.id()).collect())
            .collect();

        let mut event_handlers = Vec::with_capacity(end_of_dispatch_handlers.len());
        let mut event_reads: Vec<ResourceVec> = vec![];
        let mut event_writes: Vec<ResourceVec> = vec![];

        for handler in end_of_dispatch_handlers.into_iter().flatten() {
            let id = handler.id().0;
            for _ in 0..id - event_handlers.len() + 1 {
                event_handlers.push(None);
            }

            let event_id = handler.event_id().0;

            event_reads
                .get_mut_or_extend(event_id)
                .extend(handler.resource_reads().iter().copied());
            event_writes
                .get_mut_or_extend(event_id)
                .extend(handler.resource_writes().iter().copied());

            event_handlers[id] = Some(handler);
        }

        // We use a bounded channel because the only overhead
        // is typically on the sender's side—the receiver, the scheduler, should
        // plow through messages. This may be changed in the future.
        let (sender, receiver) = crossbeam::bounded(8);

        let bump = ThreadLocal::new();

        let starting_queue = Self::create_task_queue(&stage_systems);

        Self {
            resources,

            starting_queue,
            task_queue: VecDeque::new(), // Replaced in `execute()`

            writes_held: BitSet::new(),
            reads_held: vec![0; RESOURCE_ID_MAPPINGS.lock().len()],

            runnning_systems_count: 0,
            running_systems: BitSet::with_capacity(systems.len()),

            systems,
            stages: stage_systems,

            system_reads,
            system_writes,
            stage_reads,
            stage_writes,

            event_handlers,
            end_of_tick_handlers: construct_end_of_dispatch_handlers,

            event_reads,
            event_writes,

            bump: Arc::new(bump),

            sender,
            receiver,

            is_first_run: true,
        }
    }

    fn create_task_queue(stages: &[Stage]) -> VecDeque<Task> {
        stages
            .iter()
            .enumerate()
            .map(|(id, _)| Task::Stage(StageId(id)))
            .collect()
    }

    /// Returns the `Resources` for this scheduler.
    pub fn resources(&self) -> &Resources {
        &self.resources
    }

    /// Executes all systems and handles events.
    pub fn execute(&mut self, world: &mut World) {
        if self.is_first_run {
            self.is_first_run = false;

            self.on_first_run(world);
        }

        // Reset the task queue to the starting queue.
        self.task_queue.extend(self.starting_queue.iter().copied());

        // While there are remaining tasks, dispatch them.
        // When we encounter a task which can't be run because
        // of conflicting dependencies, we wait for tasks to
        // complete by listening on the channel.
        while let Some(task) = self.task_queue.pop_front() {
            // Attempt to run task.
            self.run_task(task, world);
        }

        // Wait for remaining systems to complete.
        while self.runnning_systems_count > 0 {
            let num = self.wait_for_completion();
            self.runnning_systems_count -= num;

            // Run any handlers/oneshots scheduled by these systems
            while let Some(task) = self.task_queue.pop_front() {
                self.run_task(task, world);
            }
        }

        // Handle `EndOfDispatch` events. Also must account
        // for these event handlers triggering further events.

        assert!(self.task_queue.is_empty());
        assert!(self.running_systems.is_empty());
    }

    fn on_first_run(&mut self, world: &mut World) {
        let sender = self.sender.clone();
        let bump = Arc::clone(&self.bump);
        let resources = &mut self.resources;

        // Initialize all systems and event handlers.
        self.systems
            .iter_mut()
            .filter(|sys| sys.is_some())
            .for_each(|sys| {
                let sys = sys.as_mut().unwrap();

                let ctx = SystemCtx {
                    sender: sender.clone(),
                    id: sys.id(),
                    bump: Arc::clone(&bump),
                };

                sys.init(resources, ctx, world);
            });

        self.event_handlers
            .iter_mut()
            .filter(|handler| handler.is_some())
            .for_each(|handler| {
                let handler = handler.as_mut().unwrap();

                let ctx = SystemCtx {
                    sender: sender.clone(),
                    id: handler.id(),
                    bump: Arc::clone(&bump),
                };

                handler.init(resources, ctx, world);
            })
    }

    /// Triggers an event manually.
    pub fn trigger<E>(&mut self, event: E, world: &mut World)
    where
        E: Event,
    {
        let event_handlers = &mut self.event_handlers;
        let end_of_tick_handlers = &self.end_of_tick_handlers;
        let slice = &[event];
        let sender = self.sender.clone();
        let bump = &self.bump;
        let resources = &self.resources;

        end_of_tick_handlers
            .get(event_id_for::<E>().0)
            .map(|handler_ids| {
                handler_ids.iter().for_each(|id| unsafe {
                    let ctx = SystemCtx {
                        id: *id,
                        sender: sender.clone(),
                        bump: Arc::clone(bump),
                    };
                    event_handlers[id.0].as_mut().unwrap().handle_raw_batch(
                        slice.as_ptr() as *const (),
                        1,
                        &resources,
                        ctx,
                        &world,
                    );
                });
            });
    }

    fn run_task(&mut self, task: Task, world: &mut World) {
        let reads = reads_for_task(
            &self.stage_reads,
            &self.system_reads,
            &self.event_reads,
            &task,
        );
        let writes = writes_for_task(
            &self.stage_writes,
            &self.system_writes,
            &self.event_writes,
            &task,
        );

        // For event handlers, we have to check that the handler is not already running, since it takes &mut self.
        let not_running = if let Task::HandleEvent(id, _, _) = &task {
            if self.end_of_tick_handlers[id.0]
                .iter()
                .any(|id| self.running_systems.contains(id.0))
            {
                Err(())
            } else {
                Ok(())
            }
        } else {
            Ok(())
        };

        match try_obtain_resources(reads, writes, &mut self.reads_held, &mut self.writes_held)
            .and(not_running)
        {
            Ok(()) => {
                // Run task and proceed.
                let systems = self.dispatch_task(task, world);
                self.runnning_systems_count += systems;
            }
            Err(()) => {
                // Execution is blocked: wait for tasks to finish.
                // Re-push the task we attempted to run to the queue.
                // TODO: optimize this
                self.task_queue.push_front(task);
                let num = self.wait_for_completion();
                self.runnning_systems_count -= num;
            }
        }
    }

    /// Waits for messages from running systems and handles them.
    ///
    /// At any point, returns with the number of systems which have completed.
    fn wait_for_completion(&mut self) -> usize {
        // Unwrap is allowed because the channel never becomes disconnected
        // (`Scheduler` holds a `Sender` handle for it).
        // This will never block indefinitely because there are always
        // systems running when this is invoked.
        let msg = self.receiver.recv().unwrap();

        match msg {
            // TODO: events
            TaskMessage::SystemComplete(id) => {
                self.release_resources_for_system(id);
                self.running_systems.remove(id.0);
                1
            }
            TaskMessage::StageComplete(id) => {
                self.release_resources_for_stage(id);
                let running_systems = &mut self.running_systems;
                self.stages[id.0].iter().for_each(|id| {
                    running_systems.remove(id.0);
                });
                self.stages[id.0].len()
            }
            TaskMessage::TriggerEvents { id, ptr, len } => {
                self.task_queue.push_back(Task::HandleEvent(id, ptr, len));
                0
            }
            TaskMessage::EventHandlingComplete(id) => {
                self.release_resources_for_event_handler(id);
                let running_systems = &mut self.running_systems;
                self.end_of_tick_handlers[id.0].iter().for_each(|id| {
                    running_systems.remove(id.0);
                });
                self.end_of_tick_handlers[id.0].len()
            }
        }
    }

    fn release_resources_for_system(&mut self, id: SystemId) {
        let reads = &self.system_reads[id.0];
        let writes = &self.system_writes[id.0];

        for read in reads {
            self.reads_held[read.0] -= 1;
        }

        for write in writes {
            self.writes_held.remove(write.0);
        }
    }

    fn release_resources_for_stage(&mut self, id: StageId) {
        for read in &self.stage_reads[id.0] {
            self.reads_held[read.0] -= 1;
        }

        for write in &self.stage_writes[id.0] {
            self.writes_held.remove(write.0);
        }
    }

    fn release_resources_for_event_handler(&mut self, id: EventId) {
        let reads = &self.event_reads[id.0];
        let writes = &self.event_writes[id.0];

        for read in reads {
            self.reads_held[read.0] -= 1;
        }

        for write in writes {
            self.writes_held.remove(write.0);
        }
    }

    /// Dispatches a task, returning the number of systems spawned.
    fn dispatch_task(&mut self, task: Task, world: &mut World) -> usize {
        match task {
            Task::Stage(id) => {
                let running_systems = &mut self.running_systems;
                self.stages[id.0].iter().for_each(|id| {
                    running_systems.insert(id.0);
                });
                self.dispatch_stage(id, world);
                self.stages[id.0].len()
            }
            Task::Oneshot(id) => {
                self.running_systems.insert(id.0);
                self.dispatch_system(id, world);
                1
            }
            Task::HandleEvent(id, ptr, len) => {
                let running_systems = &mut self.running_systems;
                let handlers = &self.end_of_tick_handlers[id.0];

                handlers.iter().for_each(|id| {
                    running_systems.insert(id.0);
                });

                self.dispatch_event_handlers(id, ptr, len, world);

                let handlers = &self.end_of_tick_handlers[id.0];
                handlers.len()
            }
        }
    }

    fn dispatch_stage(&mut self, id: StageId, world: &mut World) {
        // Rather than spawning each system independently, we optimize
        // this by running them in batch. This reduces synchronization overhead
        // with the scheduler using channels.

        // Safety of these raw pointers: they remain valid as long as the scheduler
        // is still in `execute()`, and `execute()` will not return until all systems
        // have completed.
        let stage = SharedRawPtr(&self.stages[id.0] as *const Stage);
        let resources = SharedRawPtr(&self.resources as *const Resources);

        let systems = SharedMutRawPtr(&mut self.systems as *mut Vec<Option<Box<DynSystem>>>);

        let world = SharedRawPtr(world as *const World);

        let sender = self.sender.clone();
        let bump = Arc::clone(&self.bump);

        rayon::spawn(move || {
            unsafe {
                (&*stage.0)
                    .par_iter()
                    .map(|sys_id| (sys_id, (&mut *systems.0)[sys_id.0].as_mut().unwrap()))
                    .for_each(|(sys_id, sys)| {
                        let ctx = SystemCtx {
                            id: *sys_id,
                            sender: sender.clone(),
                            bump: Arc::clone(&bump),
                        };

                        sys.execute_raw(&*resources.0, ctx, &*world.0);
                    });
            }

            // TODO: events, oneshot
            sender.send(TaskMessage::StageComplete(id)).unwrap();
        });
    }

    fn dispatch_system(&mut self, id: SystemId, world: &World) {
        let resources = SharedRawPtr(&self.resources as *const Resources);
        let world = SharedRawPtr(world as *const World);

        let system = {
            let sys = self.systems[id.0].as_mut().unwrap();
            SharedMutRawPtr(sys.as_mut() as *mut dyn RawSystem)
        };

        let ctx = self.create_system_ctx(id);

        let sender = self.sender.clone();
        rayon::spawn(move || {
            unsafe {
                // Safety: the world is not dropped while the system
                // executes, since `execute` will not return until
                // all systems have completed.
                (&mut *system.0).execute_raw(&*resources.0, ctx, &*world.0);
            }

            // TODO: events
            sender.send(TaskMessage::SystemComplete(id)).unwrap();
        });
    }

    fn dispatch_event_handlers(
        &mut self,
        id: EventId,
        ptr: *const (),
        len: usize,
        world: &mut World,
    ) {
        let handler_ids =
            SharedRawPtr(&self.end_of_tick_handlers[id.0] as *const SmallVec<[SystemId; 4]>);
        let handlers =
            SharedMutRawPtr(&mut self.event_handlers as *mut Vec<Option<Box<dyn RawEventHandler>>>);
        let resources = SharedRawPtr(&self.resources as *const Resources);
        let sender = self.sender.clone();
        let ptr = SharedRawPtr(ptr);
        let world = SharedRawPtr(world as *const World);

        let bump = Arc::clone(&self.bump);

        rayon::spawn(move || {
            // Safety: see dispatch_system().
            unsafe {
                (&*handler_ids.0)
                    .iter()
                    .map(|id| (id, (&mut *handlers.0)[id.0].as_mut().unwrap()))
                    .for_each(|(handler_id, handler)| {
                        debug_assert_eq!(handler.event_id(), id);

                        let ctx = SystemCtx {
                            id: *handler_id,
                            sender: sender.clone(),
                            bump: Arc::clone(&bump),
                        };

                        handler.handle_raw_batch(ptr.0, len, &*resources.0, ctx, &*world.0);
                    });

                sender.send(TaskMessage::EventHandlingComplete(id)).unwrap();
            }
        });
    }

    fn create_system_ctx(&self, id: SystemId) -> SystemCtx {
        SystemCtx {
            sender: self.sender.clone(),
            id,
            bump: Arc::clone(&self.bump),
        }
    }
}

/// Attempts to acquire resources for a task, returning `Err` if
/// there was a conflict and `Ok` if successful.
fn try_obtain_resources(
    reads: &ResourceVec,
    writes: &ResourceVec,
    reads_held: &mut [u32],
    writes_held: &mut BitSet,
) -> Result<(), ()> {
    // First, go through resources and confirm that there are no conflicting
    // accessors.
    // Since both read and write dependencies will only conflict with another resource
    // access when there is another write access, we can interpret them in the same way.
    for resource in reads.iter().chain(writes) {
        if writes_held.contains(resource.0) {
            return Err(()); // Conflict
        }
    }
    // Write resources will also conflict with existing read ones.
    for resource in writes {
        if reads_held[resource.0] > 0 {
            return Err(()); // Conflict
        }
    }

    // Now obtain resources by updating internal structures.
    for read in reads {
        reads_held[read.0] += 1;
    }

    for write in writes {
        writes_held.insert(write.0);
    }

    Ok(())
}

fn reads_for_task<'a>(
    stage_reads: &'a [ResourceVec],
    system_reads: &'a [ResourceVec],
    event_reads: &'a [ResourceVec],
    task: &Task,
) -> &'a ResourceVec {
    match task {
        Task::Stage(id) => &stage_reads[id.0],
        Task::Oneshot(id) => &system_reads[id.0],
        Task::HandleEvent(id, _, _) => &event_reads[id.0],
    }
}

fn writes_for_task<'a>(
    stage_writes: &'a [ResourceVec],
    system_writes: &'a [ResourceVec],
    event_writes: &'a [ResourceVec],
    task: &Task,
) -> &'a ResourceVec {
    match task {
        Task::Stage(id) => &stage_writes[id.0],
        Task::Oneshot(id) => &system_writes[id.0],
        Task::HandleEvent(id, _, _) => &event_writes[id.0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_scheduler_traits() {
        static_assertions::assert_impl_all!(Scheduler: Send, Sync);
    }
}
