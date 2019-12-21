use crate::{EventsBuilder, RawEventHandler, RawSystem, SchedulerBuilder};
use parking_lot::Mutex;

pub struct SystemRegistration(pub Mutex<Option<Box<dyn RawSystem>>>);
pub struct HandlerRegistration(pub Mutex<Option<Box<dyn RawEventHandler>>>);

inventory::collect!(SystemRegistration);
inventory::collect!(HandlerRegistration);

/// Builds a `SchedulerBuilder` with automatically registered systems.
///
/// All systems annotated with the `system` macro will automatically
/// be added to the builder. Additional systems may be added
/// manually to the returned `SchedulerBuilder`.
///
/// # Panics
/// Panics if this function has already been called.
pub fn build_scheduler() -> SchedulerBuilder {
    let mut builder = EventsBuilder::new();

    for handler in inventory::iter::<HandlerRegistration> {
        let mut guard = handler.0.lock();

        let handler = guard.take().expect("already called `build_scheduler()`");
        builder.add_boxed(handler);
    }

    let mut builder = builder.finish();

    for system in inventory::iter::<SystemRegistration> {
        let mut guard = system.0.lock();

        let system = guard.take().expect("already called `build_scheduler()`");
        builder.add_boxed(system);
    }

    builder
}
