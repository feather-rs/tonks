use crate::{RawSystem, SchedulerBuilder};
use parking_lot::Mutex;

pub struct RegisteredSystem(pub Mutex<Option<Box<dyn RawSystem>>>);

inventory::collect!(RegisteredSystem);

/// Builds a `SchedulerBuilder` with automatically registered systems.
///
/// All systems annotated with the `system` macro will automatically
/// be added to the builder. Additional systems may be added
/// manually to the returned `SchedulerBuilder`.
///
/// # Panics
/// Panics if this function has already been called.
pub fn build_scheduler() -> SchedulerBuilder {
    let mut builder = SchedulerBuilder::new();

    for system in inventory::iter::<RegisteredSystem> {
        let mut guard = system.0.lock();

        let system = guard.take().expect("already called `build_scheduler()`");
        builder.add_boxed(system);
    }

    builder
}
