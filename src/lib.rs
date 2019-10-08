#[macro_use]
extern crate derivative;

use shred::World;

mod scheduler;

pub use scheduler::{Scheduler, SchedulerBuilder};
use shred::ResourceId;
use shred::SystemData;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum EventHandleStratgy {
    Immediate,
    BeforeDependent,
    Relaxed,
}

pub trait System<'a>: Send + Sync {
    type SystemData: SystemData<'a>;

    fn run(&self, data: Self::SystemData);

    fn reads(&self) -> Vec<ResourceId> {
        Self::SystemData::reads()
    }

    fn writes(&self) -> Vec<ResourceId> {
        Self::SystemData::writes()
    }
}

pub trait RunNow<'a>: Send + Sync {
    fn run_now(&self, world: &'a World);
}

impl<'a, S> RunNow<'a> for S
where
    S: System<'a>,
{
    fn run_now(&self, world: &'a World) {
        let data = S::SystemData::fetch(world);
        self.run(data);
    }
}
