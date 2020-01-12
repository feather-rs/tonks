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

mod accessor;
mod event;
mod mappings;
mod query;
#[cfg(feature = "system-registry")]
mod registry;
mod resources;
mod scheduler;
mod system;
mod try_default;

pub use accessor::{EntityAccessor, QueryAccessor};
pub use event::{CachedEventHandler, Event, EventHandler, EventId, RawEventHandler, Trigger};
pub use query::{PreparedWorld, Query};
#[cfg(feature = "system-registry")]
pub use registry::*;
pub use resources::{resource_id_for, resource_id_for_component, ResourceId, Resources};
pub use scheduler::{EventsBuilder, Scheduler, SchedulerBuilder};
pub use system::{
    system_id_for, CachedSystem, MacroData, RawSystem, Read, System, SystemCtx, SystemData,
    SystemDataOutput, SystemId, Write,
};
pub use tonks_macros::{event_handler, system, Resource};
pub use try_default::TryDefault;
