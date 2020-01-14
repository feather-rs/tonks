//! Building of stage pipelines, which are used to organize system
//! execution order while ensuring resource borrow safety.

use crate::event::HandleStrategy;
use crate::scheduler::OrExtend;
use crate::{
    resource_id_for_component, CachedEventHandler, CachedSystem, Event, EventHandler,
    RawEventHandler, RawSystem, ResourceId, Resources, Scheduler, System,
};
use hashbrown::HashSet;
use legion::storage::ComponentTypeId;

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
        self.add_boxed(Box::new(CachedEventHandler::new(handler, "null")))
    }

    /// Adds a boxed event handler.
    pub fn add_boxed(&mut self, handler: Box<dyn RawEventHandler>) {
        assert_valid_deps(
            handler.resource_reads(),
            handler.resource_writes(),
            handler.name(),
        );

        let event_id = handler.event_id();

        let events_vec = match handler.strategy() {
            HandleStrategy::EndOfTick => &mut self.end_of_dispatch,
            _ => unimplemented!("unimplemented handle strategy"),
        };

        events_vec.get_mut_or_extend(event_id.0).push(handler);
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

/// Access to either a component or a resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Access {
    Resource(ResourceId),
    Component(ComponentTypeId),
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
        assert_valid_deps(
            system.resource_reads(),
            system.resource_writes(),
            system.name(),
        );

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
        let system = CachedSystem::new(system, "null");

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
    pub fn build(self, resources: Resources) -> Scheduler {
        let mut systems = vec![];
        let mut reads = vec![];
        let mut writes = vec![];

        for stage in self.stages {
            for system in &stage.systems {
                reads.push(system.resource_reads().to_vec());
                writes.push(system.resource_writes().to_vec());

                // Map component to resource IDs
                reads.push(
                    system
                        .component_reads()
                        .iter()
                        .map(|component| resource_id_for_component(*component))
                        .collect(),
                );
                writes.push(
                    system
                        .component_writes()
                        .iter()
                        .map(|component| resource_id_for_component(*component))
                        .collect(),
                );
            }

            systems.push(stage.systems);
        }

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
    reads: HashSet<Access>,
    /// Set of resources which are written by this stage.
    writes: HashSet<Access>,
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
            .copied()
            .any(|resource| self.writes.contains(&Access::Resource(resource)))
            || system.resource_writes().iter().copied().any(|resource| {
                self.reads.contains(&Access::Resource(resource))
                    || self.writes.contains(&Access::Resource(resource))
            })
            || system
                .component_reads()
                .iter()
                .copied()
                .any(|component| self.writes.contains(&Access::Component(component)))
            || system.component_writes().iter().copied().any(|component| {
                self.reads.contains(&Access::Component(component))
                    || self.writes.contains(&Access::Component(component))
            })
    }

    /// Adds a system to this stage.
    pub fn add(&mut self, system: Box<dyn RawSystem>) {
        system
            .resource_reads()
            .iter()
            .copied()
            .for_each(|resource| {
                self.reads.insert(Access::Resource(resource));
            });
        system
            .resource_writes()
            .iter()
            .copied()
            .for_each(|resource| {
                self.writes.insert(Access::Resource(resource));
            });
        system
            .component_reads()
            .iter()
            .copied()
            .for_each(|component| {
                self.reads.insert(Access::Component(component));
            });
        system
            .component_writes()
            .iter()
            .copied()
            .for_each(|component| {
                self.writes.insert(Access::Component(component));
            });
        self.systems.push(system);
    }
}

fn assert_valid_deps(reads: &[ResourceId], writes: &[ResourceId], name: &str) {
    // Verify that there are no conflicts in the system's own resource access.
    // This prevents UB such as mutable aliasing.
    assert!(
        reads.iter().all(|resource| !writes.contains(resource)),
        "system {} cannot read and write same resource",
        name
    );
    let valid_mutable = writes.iter().all(|resource| {
        !reads.contains(resource) && writes.iter().filter(|res| *res == resource).count() == 1
    });
    assert!(
        valid_mutable,
        "system {} cannot have double mutable access to the same resource",
        name
    );
}
