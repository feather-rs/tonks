#[macro_use]
extern crate derivative;
#[macro_use]
extern crate mopa;
#[macro_use]
extern crate static_assertions;

mod event;
mod mappings;
mod query;
mod resources;
mod scheduler;
mod system;

pub use event::{CachedEventHandler, Event, EventHandler, EventId, RawEventHandler, Trigger};
pub use query::{PreparedQuery, PreparedWorld, Query};
pub use resources::{resource_id_for, ResourceId, Resources};
pub use scheduler::{EventsBuilder, Scheduler, SchedulerBuilder};
pub use system::{
    system_id_for, CachedSystem, RawSystem, Read, System, SystemData, SystemId, Write,
};
