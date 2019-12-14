//! Building of stage pipelines, which are used to organize system
//! execution order while ensuring resource borrow safety.

use crate::event::HandleStrategy;
use crate::scheduler::OrExtend;
use crate::{
    CachedEventHandler, CachedSystem, Event, EventHandler, RawEventHandler, RawSystem, ResourceId,
    Resources, Scheduler, System,
};
use hashbrown::HashSet;

/// Builder of event pipelines.
#[derive(Default)]
pub struct EventsBuilder {
    /// Vector of end-of-dispatch event handlers.
    ///
    /// This vector is indexed by the `EventId`.
    end_of_dispatch: Vec<Vec<Box<dyn RawEventHandler>>>,
}

impl EventsBuilder {
    /// Creates a new `EventsBuilder` with no event handlers.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an event handler.
    pub fn add<H, E>(&mut self, handler: H)
    where
        H: EventHandler<E>,
        E: Event,
    {
        let handler = CachedEventHandler::new(handler);

        assert_valid_deps(handler.resource_reads(), handler.resource_writes());

        let event_id = handler.event_id();

        let events_vec = match handler.strategy() {
            HandleStrategy::EndOfTick => &mut self.end_of_dispatch,
            _ => unimplemented!("unimplemented handle strategy"),
        };

        events_vec
            .get_mut_or_extend(event_id.0)
            .push(Box::new(handler));
    }

    /// Adds an event handler to this builder, returning the `EventsBuilder`
    /// for method chaining.
    pub fn with<H, E>(mut self, handler: H) -> Self
    where
        H: EventHandler<E>,
        E: Event,
    {
        self.add(handler);
        self
    }

    /// Finishes construction of this events builder, returning a `SchedulerBuilder`
    /// which can be used to further add systems.
    pub fn finish(self) -> SchedulerBuilder {
        SchedulerBuilder {
            stages: vec![],
            events: self,
        }
    }
}

/// Builder of a stage pipeline.
#[derive(Default)]
pub struct SchedulerBuilder {
    /// Stages which have been created so far. New systems can
    /// be inserted into existing stages or be added in a new stage.
    stages: Vec<Stage>,
    events: EventsBuilder,
}

impl SchedulerBuilder {
    /// Creates a new `StageBuilder` with no systems.
    ///
    /// If you want to use event handlers, build an `EventsBuilder`
    /// and call `finish()` on it to create a `SchedulerBuilder`
    /// with those events registered.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a boxed system to the stage pipeline.
    pub fn add_boxed(&mut self, system: Box<dyn RawSystem>) {
        assert_valid_deps(system.resource_reads(), system.resource_writes());

        if let Some(stage) = self
            .stages
            .iter_mut()
            .find(|stage| !stage.conflicts_with(&*system))
        {
            stage.add(system);
        } else {
            // Create new stage.
            let mut new_stage = Stage::new();
            new_stage.add(system);
            self.stages.push(new_stage);
        }
    }

    /// Adds a system to the stage pipeline.
    pub fn add<S: System + 'static>(&mut self, system: S) {
        let system = CachedSystem::new(system);

        self.add_boxed(Box::new(system));
    }

    /// Adds a system to the stage pipeline, returning
    /// the `StageBuilder` for method chaining.
    pub fn with<S: System + 'static>(mut self, system: S) -> Self {
        self.add(system);
        self
    }

    /// Creates a new `Scheduler` based on the stage pipeline
    /// which was built.
    pub fn build(self, mut resources: Resources) -> Scheduler {
        let mut systems = vec![];
        let mut reads = vec![];
        let mut writes = vec![];

        for stage in self.stages {
            for system in &stage.systems {
                reads.push(system.resource_reads().to_vec());
                writes.push(system.resource_writes().to_vec());
            }

            systems.push(stage.systems);
        }

        systems
            .iter_mut()
            .flatten()
            .for_each(|system| system.init(&mut resources));

        // Safety: the builder must work correctly to ensure
        // that stages are correct.
        unsafe {
            Scheduler::new(
                systems,
                self.events.end_of_dispatch,
                reads,
                writes,
                resources,
            )
        }
    }
}

/// A stage of a stage builder.
struct Stage {
    /// Vector of items in this stage.
    systems: Vec<Box<dyn RawSystem>>,
    /// Set of resources which are read by this stage.
    reads: HashSet<ResourceId>,
    /// Set of resources which are written by this stage.
    writes: HashSet<ResourceId>,
}

impl Default for Stage {
    fn default() -> Self {
        Self {
            systems: vec![],
            reads: HashSet::new(),
            writes: HashSet::new(),
        }
    }
}

impl Stage {
    /// Creates a new, empty stage.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns whether the given system conflicts with this stage.
    pub fn conflicts_with(&self, system: &dyn RawSystem) -> bool {
        system
            .resource_reads()
            .iter()
            .any(|resource| self.writes.contains(resource))
            || system
                .resource_writes()
                .iter()
                .any(|resource| self.reads.contains(resource) || self.writes.contains(resource))
    }

    /// Adds a system to this stage.
    pub fn add(&mut self, system: Box<dyn RawSystem>) {
        system.resource_reads().iter().for_each(|resource| {
            self.reads.insert(*resource);
        });
        system.resource_writes().iter().for_each(|resource| {
            self.writes.insert(*resource);
        });
        self.systems.push(system);
    }
}

fn assert_valid_deps(reads: &[ResourceId], writes: &[ResourceId]) {
    // Verify that there are no conflicts in the system's own resource access.
    // This prevents UB such as mutable aliasing.
    assert!(
        reads.iter().all(|resource| !writes.contains(resource)),
        "system cannot read and write same resource"
    );
    let valid_mutable = writes.iter().all(|resource| {
        !reads.contains(resource) && writes.iter().filter(|res| *res == resource).count() == 1
    });
    assert!(
        valid_mutable,
        "system cannot have double mutable access to the same resource"
    );
}
