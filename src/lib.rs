#[macro_use]
extern crate derivative;
#[macro_use]
extern crate mopa;

mod mappings;
mod resources;
mod scheduler;
mod system;

pub use resources::{resource_id_for, ResourceId, Resources};
pub use scheduler::{Scheduler, SchedulerBuilder};
pub use system::{
    system_id_for, CachedSystem, RawSystem, Read, System, SystemData, SystemId, Write,
};
