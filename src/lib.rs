#![feature(specialization)]

#[macro_use]
extern crate derivative;
#[macro_use]
extern crate mopa;
#[macro_use]
extern crate static_assertions;

#[cfg(feature = "system-registry")]
pub extern crate inventory;
#[cfg(feature = "system-registry")]
pub extern crate parking_lot;

mod event;
mod mappings;
mod query;
#[cfg(feature = "system-registry")]
mod registry;
mod resources;
mod scheduler;
mod system;
mod try_default;

pub use event::{CachedEventHandler, Event, EventHandler, EventId, RawEventHandler, Trigger};
pub use query::{PreparedWorld, Query};
#[cfg(feature = "system-registry")]
pub use registry::*;
pub use resources::{resource_id_for, ResourceId, Resources};
pub use scheduler::{EventsBuilder, Scheduler, SchedulerBuilder};
pub use system::{
    system_id_for, CachedSystem, MacroData, RawSystem, Read, System, SystemData, SystemDataOutput,
    SystemId, Write,
};
pub use tonks_macros::{system, Resource};
pub use try_default::TryDefault;
