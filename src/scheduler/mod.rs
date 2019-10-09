use crate::RunNow;
use bit_set::BitSet;
use bumpalo::Bump;
use hashbrown::{HashMap, HashSet};
use shred::{ResourceId, World};
use smallvec::smallvec;
use smallvec::SmallVec;
use std::collections::VecDeque;
use std::sync::Arc;
use thread_local::ThreadLocal;

mod atomic_bit_set;
mod builder;

pub use atomic_bit_set::AtomicBitSet;
pub use builder::SchedulerBuilder;
use std::cell::RefCell;
use std::iter;

/// Internal ID used to identify resources.
/// These are allocated consecutively so that
/// they can be used as indices into vectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct InternalResourceId(usize);
/// Internal ID used to identify systems.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct SystemId(usize);
/// Internal ID used to identify stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct StageId(usize);
/// Internal ID used to identify event types.
/// These are allocated consecutively so that
/// they can be used as indices into vectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct EventId(usize);

/// A stage in the completion of a dispatch. Each stage
/// contains systems which can be executed in parallel.
type Stage = SmallVec<[SystemId; 6]>;

type ResourceVec = SmallVec<[InternalResourceId; 8]>;

/// Raw pointer to the world.
struct WorldPtr(*const World);

// We can implement Send and Sync for this because
// during the execution of a system which holds
// a reference to `WorldPtr`, the scheduler continues
// to run. During this time, the scheduler holds
// a shared reference to `World`, and thus
// it cannot mutate or drop it.
unsafe impl Send for WorldPtr {}
unsafe impl Sync for WorldPtr {}

/// Raw pointer to a system.
struct SystemPtr(*const Arc<dyn RunNow<'static>>);

impl SystemPtr {
    unsafe fn from_arc<'a>(arc: &Arc<dyn RunNow<'a>>) -> Self {
        // Transmute to static lifetime.
        let arc = std::mem::transmute::<_, &Arc<dyn RunNow<'static>>>(arc);
        Self(arc as *const Arc<dyn RunNow<'static>>)
    }
}

// Same applies as above.
unsafe impl Send for SystemPtr {}
unsafe impl Sync for SystemPtr {}

struct SystemMessagePtr(*mut Option<SystemMessage>);

unsafe impl Send for SystemMessagePtr {}
unsafe impl Sync for SystemMessagePtr {}

/// A message sent from a running system to the scheduler.
enum SystemMessage {
    /// A system completed execution. Any non-immediately executed
    /// events are sent in addition so that the scheduler can queue these.
    ///
    /// We send the events in the same message as the completion message
    /// to reduce channel send overhead.
    Completed {
        /// The ID of the completed system.
        id: SystemId,
        /// Vector of events which this system executed.
        ///
        /// # Safety
        /// Each pointer __must__ point to a valid event with the same
        /// type corresponding to the `InternalEventId`. Each pointer __must__
        /// be allocated in one of the thread-local bump allocators to ensure
        /// they are deallocated on dispatch end.
        // An inline size of 3 is used to attempt to keep this type
        // within one cache line.
        events: SmallVec<[(*mut (), EventId); 3]>,
    },
}

unsafe impl Send for SystemMessage {}

/// A task to run. This can either be a stage (mutliple systems run in parallel),
/// a oneshot system, or an event handling pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Task {
    Stage(StageId),
    Oneshot(SystemId),
    // TODO: event pipeline
}

/// The `tonks` scheduler. This is similar to `shred::Dispatcher`
/// but has more features.
#[derive(Derivative)]
#[derivative(Debug)]
pub struct Scheduler<'a> {
    /// Queue of tasks to run during the current dispatch.
    ///
    /// Each dispatch, this queue is reset to `starting_queue`.
    task_queue: VecDeque<Task>,
    /// The starting task queue for any dispatch.
    starting_queue: VecDeque<Task>,

    /// Set of currently running systems.
    running_systems: RefCell<HashSet<SystemId>>,

    /// Bit set representing write resources which are currently held.
    ///
    /// This set is indexed by the `InternalResourceId`.
    writes_held: BitSet,
    /// Vector of reference counts representing the number of tasks currently
    /// holding a shared reference to a resource.
    ///
    /// This vector is indexed by the `InternalResourceId`.
    reads_held: Vec<u8>,

    /// Mapping from `shred::ResourceId` to `InternalResourceId`.
    resource_id_mappings: HashMap<ResourceId, InternalResourceId>,

    /// Thread-local bump allocator used to allocate events.
    ///
    /// TODO: implement a lock-free bump arena instead.
    #[derivative(Debug = "ignore")]
    bump: ThreadLocal<Bump>,

    /// Vector of systems which can be executed. This includes oneshottable
    /// systems as well.
    ///
    /// This vector is indexed by the `SystemId`.
    #[derivative(Debug = "ignore")]
    systems: Vec<Arc<(dyn RunNow<'a> + 'static)>>,
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

    /// Bit set with set bits for each system which has completed
    /// executing. This is cleared at the end of every dispatch.
    ///
    /// This set is indexed by the `SystemId`.
    #[derivative(Debug = "ignore")]
    finished: Arc<AtomicBitSet>,
    /// Vector of `SystemMessage`s received from completed systems.
    /// Upon completion, systems will set their value in this
    /// vector to their desired message before setting their
    /// bit in `finished`.
    ///
    /// This vector is indexed by the `SystemId`.
    #[derivative(Debug = "ignore")]
    messages: RefCell<Vec<Option<SystemMessage>>>,
}

impl<'a> Scheduler<'a> {
    /// Creates a new `Scheduler` with the given stages.
    ///
    /// `deps` is a vector indexed by the system ID containing
    /// resources for each system.
    ///
    /// # Contract
    /// The stages are assumed to have been assembled correctly:
    /// no two systems in a stage may conflict with each other.
    fn new(
        stages: Vec<Vec<Box<(dyn RunNow<'a> + 'static)>>>,
        read_deps: Vec<Vec<ResourceId>>,
        write_deps: Vec<Vec<ResourceId>>,
    ) -> Self {
        // Assign numerical IDs to resources.
        let mut resource_id_mappings = HashMap::new();
        let mut counter = 0;
        for dep in read_deps
            .iter()
            .flatten()
            .chain(write_deps.iter().flatten())
        {
            resource_id_mappings.entry(dep.clone()).or_insert_with(|| {
                let r = counter;
                counter += 1;
                InternalResourceId(r)
            });
        }

        // Detect resources used by systems and create those vectors.
        // Also collect systems into uniform vector.
        let mut system_reads: Vec<ResourceVec> = vec![];
        let mut system_writes: Vec<ResourceVec> = vec![];
        let mut stage_reads: Vec<ResourceVec> = vec![];
        let mut stage_writes: Vec<ResourceVec> = vec![];
        let mut systems = vec![];
        let mut stage_systems = vec![];

        counter = 0;
        for stage in stages {
            let mut stage_read = vec![];
            let mut stage_write = vec![];
            let mut systems_in_stage = smallvec![];

            for system in stage {
                system_reads.push(
                    read_deps[counter]
                        .clone()
                        .into_iter()
                        .map(|id| resource_id_mappings[&id])
                        .collect(),
                );
                system_writes.push(
                    write_deps[counter]
                        .clone()
                        .into_iter()
                        .map(|id| resource_id_mappings[&id])
                        .collect(),
                );
                stage_read.extend(system_reads[counter].clone());
                stage_write.extend(system_writes[counter].clone());
                systems.push(system.into());
                systems_in_stage.push(SystemId(counter));
                counter += 1;
            }

            stage_reads.push(stage_read.into_iter().collect());
            stage_writes.push(stage_write.into_iter().collect());
            stage_systems.push(systems_in_stage);
        }

        let bump = ThreadLocal::new();

        let starting_queue = Self::create_task_queue(&stage_systems);

        let num_systems = systems.len();

        Self {
            starting_queue,
            task_queue: VecDeque::new(), // Replaced in `execute()`

            writes_held: BitSet::with_capacity(resource_id_mappings.len()),
            reads_held: vec![0; resource_id_mappings.len()],

            running_systems: RefCell::new(HashSet::with_capacity(16)),

            systems,
            stages: stage_systems,

            system_reads,
            system_writes,
            stage_reads,
            stage_writes,

            bump,

            finished: Arc::new(AtomicBitSet::with_capacity(num_systems)),
            messages: RefCell::new(iter::repeat_with(|| None).take(num_systems).collect()),
            resource_id_mappings,
        }
    }

    fn create_task_queue(stages: &[Stage]) -> VecDeque<Task> {
        stages
            .iter()
            .enumerate()
            .map(|(id, _)| Task::Stage(StageId(id)))
            .collect()
    }

    /// Executes all systems and handles events.
    pub fn execute(&mut self, world: &World) {
        // Reset the task queue to the starting queue.
        self.task_queue = self.starting_queue.clone();

        // While there are remaining tasks, dispatch them.
        // When we encounter a task which can't be run because
        // of conflicting dependencies, we wait for tasks to
        // complete by listening on the channel.
        while let Some(task) = self.task_queue.pop_front() {
            // Attempt to run task.
            let reads = reads_for_task(&self.stage_reads, &self.system_reads, &task);
            let writes = writes_for_task(&self.stage_writes, &self.system_writes, &task);

            match try_obtain_resources(reads, writes, &mut self.reads_held, &mut self.writes_held) {
                Ok(()) => {
                    // Run task and proceed.
                    self.dispatch_task(task, world);
                }
                Err(()) => {
                    // Execution is blocked: wait for tasks to finish.
                    // Re-push the task we attempted to run to the queue.
                    // TODO: optimize this
                    self.task_queue.push_front(task);
                    self.wait_for_completion();
                }
            }
        }

        // Wait for remaining systems to complete.
        while !self.running_systems.borrow().is_empty() {
            self.wait_for_completion();
        }

        self.finished.clear();
    }

    /// Waits until a task has completed, also submitting
    /// any events or oneshot systems requested by the
    /// task to the task queue.
    fn wait_for_completion(&mut self) {
        // Wait for one or more systems to be completed.
        self.finished.wait_on_update();

        // Go through all running systems and check if they completed.
        let mut completed: SmallVec<[_; 4]> = smallvec![];
        for system in self.running_systems.borrow().iter() {
            if self.finished.contains(system.0) {
                completed.push(*system);
                let msg = self.messages.borrow_mut()[system.0].take().unwrap();

                match msg {
                    // TODO: events
                    SystemMessage::Completed { .. } => (),
                }
            }
        }

        completed.into_iter().for_each(|system| {
            self.release_resources_for_system(system);
            self.running_systems.borrow_mut().remove(&system);
        });
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

    /// Dispatches a task, returning the number of systems spawned.
    fn dispatch_task(&self, task: Task, world: &World) -> usize {
        match task {
            Task::Stage(id) => {
                let systems = &self.stages[id.0];
                let len = systems.len();
                systems
                    .iter()
                    .for_each(|sys| self.dispatch_system(*sys, world));
                len
            }
            Task::Oneshot(id) => {
                self.dispatch_system(id, world);
                1
            }
        }
    }

    fn dispatch_system(&self, id: SystemId, world: &World) {
        let sys = unsafe { SystemPtr::from_arc(&self.systems[id.0]) };
        let world = WorldPtr(world as *const World);

        let finished = Arc::clone(&self.finished);

        // Safety: the same system does not run at the same time,
        // so there is never more than one reference to this
        // message.
        let msg =
            SystemMessagePtr((&mut self.messages.borrow_mut()[id.0]) as *mut Option<SystemMessage>);

        self.running_systems.borrow_mut().insert(id);

        rayon::spawn(move || {
            unsafe {
                // Safety: the world is not dropped while the system
                // executes, since `execute` will not return until
                // all systems have completed.
                (&*(sys.0)).run_now(&*(world.0));

                *(msg.0) = Some(SystemMessage::Completed {
                    id,
                    events: smallvec![],
                });
            }

            finished.insert(id.0, true);
        });
    }
}

/// Attempts to acquire resources for a task, returning `Err` if
/// there was a conflict and `Ok` if successful.
fn try_obtain_resources(
    reads: &ResourceVec,
    writes: &ResourceVec,
    reads_held: &mut [u8],
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
    task: &Task,
) -> &'a ResourceVec {
    match task {
        Task::Stage(id) => &stage_reads[id.0],
        Task::Oneshot(id) => &system_reads[id.0],
    }
}

fn writes_for_task<'a>(
    stage_writes: &'a [ResourceVec],
    system_writes: &'a [ResourceVec],
    task: &Task,
) -> &'a ResourceVec {
    match task {
        Task::Stage(id) => &stage_writes[id.0],
        Task::Oneshot(id) => &system_writes[id.0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_scheduler_traits() {
        static_assertions::assert_impl_all!(Scheduler: Send);
    }
}
