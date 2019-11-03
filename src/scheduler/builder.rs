//! Building of stage pipelines, which are used to organize system
//! execution order while ensuring resource borrow safety.

use crate::{CachedSystem, RawSystem, ResourceId, Resources, Scheduler, System};
use hashbrown::HashSet;

/// Builder of a stage pipeline.
pub struct SchedulerBuilder {
    /// Stages which have been created so far. New systems can
    /// be inserted into existing stages or be added in a new stage.
    stages: Vec<Stage>,
}

impl Default for SchedulerBuilder {
    fn default() -> Self {
        Self { stages: vec![] }
    }
}

impl SchedulerBuilder {
    /// Creates a new `StageBuilder` with no systems.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a system to the stage pipeline.
    pub fn add<S: System + 'static>(&mut self, system: S) {
        let system = CachedSystem::new(system);

        // Verify that there are no conflicts in the system's own resource access.
        // This prevents UB such as mutable aliasing.
        assert!(
            system
                .resource_reads()
                .iter()
                .all(|resource| !system.resource_writes().contains(resource)),
            "system cannot read and write same resource"
        );
        let valid_mutable = system.resource_writes().iter().all(|resource| {
            !system.resource_reads().contains(resource)
                && system
                    .resource_writes()
                    .iter()
                    .filter(|res| *res == resource)
                    .count()
                    == 1
        });
        assert!(
            valid_mutable,
            "system cannot have double mutable access to the same resource"
        );

        if let Some(stage) = self
            .stages
            .iter_mut()
            .find(|stage| !stage.conflicts_with(&system))
        {
            stage.add(system);
        } else {
            // Create new stage.
            let mut new_stage = Stage::new();
            new_stage.add(system);
            self.stages.push(new_stage);
        }
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
            }

            systems.push(stage.systems);
        }

        Scheduler::new(systems, reads, writes, resources)
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
    pub fn conflicts_with<S: RawSystem>(&self, system: &S) -> bool {
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
    pub fn add<S: RawSystem + 'static>(&mut self, system: S) {
        system.resource_reads().iter().for_each(|resource| {
            self.reads.insert(*resource);
        });
        system.resource_writes().iter().for_each(|resource| {
            self.writes.insert(*resource);
        });
        self.systems.push(Box::new(system));
    }
}
