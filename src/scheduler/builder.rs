//! Building of stage pipelines, which are used to organize system
//! execution order while ensuring resource borrow safety.

use crate::{RawSystem, ResourceId};
use hashbrown::HashSet;

/// Implemented on types which can be organized into a pipeline of stages.
pub trait StageBuildable {
    fn reads(&self) -> &[ResourceId];
    fn writes(&self) -> &[ResourceId];
}

impl<S> StageBuildable for S
where
    S: RawSystem,
{
    fn reads(&self) -> &[ResourceId] {
        self.resource_reads()
    }

    fn writes(&self) -> &[ResourceId] {
        self.resource_writes()
    }
}

/// Builder of a stage pipeline.
pub struct StageBuilder<S: StageBuildable> {
    /// Stages which have been created so far. New `StageBuildable`s can
    /// be inserted into existing stages or be added in a new stage.
    stages: Vec<Stage<S>>,
}

impl<S> Default for StageBuilder<S>
where
    S: StageBuildable,
{
    fn default() -> Self {
        Self { stages: vec![] }
    }
}

impl<S> StageBuilder<S>
where
    S: StageBuildable,
{
    /// Creates a new `StageBuilder` with no systems.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a system to the stage pipeline.
    pub fn add(&mut self, system: S) {
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
    pub fn with(mut self, system: S) -> Self {
        self.add(system);
        self
    }
}

/// A stage of a stage builder.
struct Stage<S: StageBuildable> {
    /// Vector of items in this stage.
    systems: Vec<S>,
    /// Set of resources which are read by this stage.
    reads: HashSet<ResourceId>,
    /// Set of resources which are written by this stage.
    writes: HashSet<ResourceId>,
}

impl<S> Default for Stage<S>
where
    S: StageBuildable,
{
    fn default() -> Self {
        Self {
            systems: vec![],
            reads: HashSet::new(),
            writes: HashSet::new(),
        }
    }
}

impl<S> Stage<S>
where
    S: StageBuildable,
{
    /// Creates a new, empty stage.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns whether the given system conflicts with this stage.
    pub fn conflicts_with(&self, system: &S) -> bool {
        system
            .reads()
            .iter()
            .all(|resource| !self.writes.contains(resource))
            && system
                .writes()
                .iter()
                .all(|resource| !self.reads.contains(resource) && !self.writes.contains(resource))
    }

    /// Adds a system to this stage.
    pub fn add(&mut self, system: S) {
        system.reads().iter().for_each(|resource| {
            self.reads.insert(*resource);
        });
        system.writes().iter().for_each(|resource| {
            self.writes.insert(*resource);
        });
        self.systems.push(system);
    }
}
