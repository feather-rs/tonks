use crate::RunNow;
use bit_set::BitSet;
use bumpalo::Bump;
use crossbeam::{Receiver, Sender};
use hashbrown::HashMap;
use rayon::prelude::*;
use shred::{ResourceId, World};
use smallvec::smallvec;
use smallvec::SmallVec;
use std::collections::VecDeque;
use std::sync::Arc;
use thread_local::ThreadLocal;

mod builder;

pub use builder::SchedulerBuilder;
use std::mem;

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

/// A raw pointer to some `T`.
///
/// # Safety
/// This type implements `Send` and `Sync`, but it is
/// up to the user to ensure that:
/// * If the pointer is dereferenced, the value has not been dropped.
/// * No data races occur.
struct SharedRawPtr<T>(*const T);

unsafe impl<T: Send> Send for SharedRawPtr<T> {}
unsafe impl<T: Sync> Sync for SharedRawPtr<T> {}

/*
/// A mutable raw pointer to some `T`.
///
/// # Safety
/// This type implements `Send` and `Sync`, but it is
/// up to the user to ensure that:
/// * If the pointer is dereferenced, the value has not been dropped.
/// * No data races occur.
struct SharedMutRawPtr<T>(*mut T);

unsafe impl<T: Send> Send for SharedMutRawPtr<T> {}
unsafe impl<T: Sync> Sync for SharedMutRawPtr<T> {}
*/

/// A message sent from a running task to the scheduler.
enum TaskMessage {
    SystemComplete(SystemId),
    StageComplete(StageId),
}

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

    /// Number of currently running systems.
    runnning_systems: usize,

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

    /// Receiving end of the channel used to communicate with running systems.
    #[derivative(Debug = "ignore")]
    receiver: Receiver<TaskMessage>,
    /// Sending end of the above channel. This can be cloned and sent to systems.
    #[derivative(Debug = "ignore")]
    sender: Sender<TaskMessage>,
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

        // We use a bounded channel of capacity 1 because the only overhead
        // is typically on the sender's side—the receiver, the scheduler, should
        // plow through messages. This may be changed in the future.
        let (sender, receiver) = crossbeam::bounded(1);

        let bump = ThreadLocal::new();

        let starting_queue = Self::create_task_queue(&stage_systems);

        Self {
            starting_queue,
            task_queue: VecDeque::new(), // Replaced in `execute()`

            writes_held: BitSet::with_capacity(resource_id_mappings.len()),
            reads_held: vec![0; resource_id_mappings.len()],

            runnning_systems: 0,

            systems,
            stages: stage_systems,

            system_reads,
            system_writes,
            stage_reads,
            stage_writes,

            bump,

            sender,
            receiver,
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
                    let systems = self.dispatch_task(task, world);
                    self.runnning_systems += systems;
                }
                Err(()) => {
                    // Execution is blocked: wait for tasks to finish.
                    // Re-push the task we attempted to run to the queue.
                    // TODO: optimize this
                    self.task_queue.push_front(task);
                    let num = self.wait_for_completion();
                    self.runnning_systems -= num;
                }
            }
        }

        // Wait for remaining systems to complete.
        while self.runnning_systems > 0 {
            let num = self.wait_for_completion();
            self.runnning_systems -= num;
        }
    }

    /// Waits until one or more systems have completed, returning
    /// the number of systems registered.
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
                1
            }
            TaskMessage::StageComplete(id) => {
                self.release_resources_for_stage(id);
                self.stages[id.0].len()
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

    /// Dispatches a task, returning the number of systems spawned.
    fn dispatch_task(&self, task: Task, world: &World) -> usize {
        match task {
            Task::Stage(id) => {
                self.dispatch_stage(id, world);
                self.stages[id.0].len()
            }
            Task::Oneshot(id) => {
                self.dispatch_system(id, world);
                1
            }
        }
    }

    fn dispatch_stage(&self, id: StageId, world: &World) {
        // Rather than spawning each system independently, we optimize
        // this by running them in batch. This reduces synchronization overhead
        // with the scheduler using channels.
        let stage = SharedRawPtr(&self.stages[id.0] as *const Stage);
        let world = SharedRawPtr(world as *const World);

        let systems = unsafe {
            mem::transmute::<
                &Vec<Arc<(dyn RunNow<'a> + 'static)>>,
                &Vec<Arc<(dyn RunNow<'static> + 'static)>>,
            >(&self.systems)
        };
        let systems = SharedRawPtr(systems as *const Vec<_>);

        let sender = self.sender.clone();

        rayon::spawn(move || {
            unsafe {
                (&*stage.0)
                    .par_iter()
                    .map(|sys_id| &(&*systems.0)[sys_id.0])
                    .for_each(|sys: &Arc<(dyn RunNow + 'static)>| sys.run_now(&*(world.0)));
            }

            // TODO: events, oneshot
            sender.send(TaskMessage::StageComplete(id)).unwrap();
        });
    }

    fn dispatch_system(&self, id: SystemId, world: &World) {
        // `RunNow` has a lifetime parameter 'a, but
        // it is only used to ensure that the `SystemData`
        // outlives the `World`.
        // Because the `World` will remain valid while the system
        // is running (`execute()` does not return until all systems
        // complete), we can safely cast `RunNow<'a>` to `RunNow<'static>`.
        let sys = unsafe {
            Arc::clone(mem::transmute::<
                &Arc<dyn RunNow<'a>>,
                &Arc<dyn RunNow<'static>>,
            >(&self.systems[id.0]))
        };
        let world = SharedRawPtr(world as *const World);

        let sender = self.sender.clone();
        rayon::spawn(move || {
            unsafe {
                // Safety: the world is not dropped while the system
                // executes, since `execute` will not return until
                // all systems have completed.
                sys.run_now(&*(world.0));
            }

            // TODO: events
            sender.send(TaskMessage::SystemComplete(id)).unwrap();
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
        static_assertions::assert_impl_all!(Scheduler: Send, Sync);
    }
}
