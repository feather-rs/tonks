use crate::scheduler::Scheduler;
use crate::RunNow;
use crate::System;
use hashbrown::HashSet;
use shred::ResourceId;

/// Builder for a `Scheduler`. This is responsible
/// for segmenting `System`s into stages based on their
/// resource requirements.
#[derive(Default)]
pub struct SchedulerBuilder<'a> {
    stages: Vec<Stage<'a>>,
    known_systems: HashSet<String>,
}

impl<'a> SchedulerBuilder<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with<S: System<'a> + 'static>(
        mut self,
        sys: S,
        name: &str,
        runs_after: &[&str],
    ) -> Self {
        self.add(sys, name, runs_after);
        self
    }

    pub fn add<S: System<'a> + 'static>(&mut self, sys: S, name: &str, runs_after: &[&str]) {
        // Ensure that runs_after rules already exist
        runs_after.iter().for_each(|sys| {
            assert!(
                self.known_systems.contains(*sys),
                "System {} not yet registered",
                *sys
            )
        });

        if name != "" {
            assert!(
                !self.known_systems.contains(name),
                "System {} already exists",
                name
            );
        }

        self.known_systems.insert(String::from(name));

        let reads: HashSet<_> = sys.reads().into_iter().collect();
        let writes: HashSet<_> = sys.writes().into_iter().collect();

        // Iterate over stages in `self` and attempt to find a stage which does not
        // conflict with this system. If none are found, append a new stage.
        let stage = {
            let mut result = None;
            for (index, stage) in self.stages.iter_mut().enumerate() {
                if stage.writes.intersection(&writes).count() > 0 {
                    // Write resource conflict
                    continue;
                }

                if stage.writes.intersection(&reads).count() > 0 {
                    // Read resource conflict
                    continue;
                }

                // Check runs_after relationships
                if runs_after
                    .iter()
                    .any(|after| stage.system_names.contains(*after))
                {
                    continue;
                }

                // All checks succeeded: this stage will work
                result = Some(index);
            }

            match result {
                Some(stage) => stage,
                None => {
                    // No stage found: create one.
                    self.stages.push(Stage::default());
                    self.stages.len() - 1
                }
            }
        };

        // Add data to stage.
        let stage = &mut self.stages[stage];
        reads.iter().for_each(|read| {
            stage.reads.insert(read.clone());
        });
        writes.iter().for_each(|write| {
            stage.writes.insert(write.clone());
        });
        stage.system_names.insert(String::from(name));
        stage.systems.push(Box::new(sys));
        stage.system_reads.push(reads.into_iter().collect());
        stage.system_writes.push(writes.into_iter().collect());
    }

    pub fn build(self) -> Scheduler<'a> {
        let mut stages = Vec::with_capacity(self.stages.len());
        let mut read_deps = vec![];
        let mut write_deps = vec![];

        for stage in self.stages {
            stages.push(stage.systems);

            stage
                .system_reads
                .into_iter()
                .for_each(|reads| read_deps.push(reads));
            stage
                .system_writes
                .into_iter()
                .for_each(|writes| write_deps.push(writes));
        }

        Scheduler::new(stages, read_deps, write_deps)
    }
}

#[derive(Default)]
struct Stage<'a> {
    reads: HashSet<ResourceId>,
    writes: HashSet<ResourceId>,
    systems: Vec<Box<dyn RunNow<'a>>>,
    system_reads: Vec<Vec<ResourceId>>,
    system_writes: Vec<Vec<ResourceId>>,
    system_names: HashSet<String>,
}
