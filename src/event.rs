use crate::mappings::Mappings;
use crate::system::SYSTEM_ID_MAPPINGS;
use crate::{ResourceId, Resources, SystemData, SystemId};
use lazy_static::lazy_static;
use parking_lot::Mutex;
use std::any::TypeId;

/// ID of an event type, allocated consecutively.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct EventId(pub usize);

impl From<usize> for EventId {
    fn from(x: usize) -> Self {
        Self(x)
    }
}

lazy_static! {
    pub static ref EVENT_ID_MAPPINGS: Mutex<Mappings<TypeId, EventId>> =
        Mutex::new(Mappings::new());
}

/// Returns the event ID for the given type.
pub fn event_id_for<E>() -> EventId
where
    E: Event,
{
    EVENT_ID_MAPPINGS.lock().get_or_alloc(TypeId::of::<E>())
}

/// Marker trait for types which can be triggered as events.
pub trait Event: Send + Sync + 'static {}

impl<T> Event for T where T: Send + Sync + 'static {}

/// Strategy used to handle an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HandleStrategy {
    /*/// The handler will be invoked in the call to `trigger` so that
    /// the system triggering it will observe any side effects from
    /// handling the event.
    ///
    /// This is the default strategy.
    Immediate,*/
    /// The handler will be run at the end of the system which triggered the event.
    ///
    /// This is the default strategy.
    EndOfSystem,
    /// The handle will be scheduled for running at the end of tick.
    EndOfTick,
}

impl Default for HandleStrategy {
    fn default() -> Self {
        HandleStrategy::EndOfSystem
    }
}

/// A raw event handler.
///
/// # Safety
/// * The event type returned by `event_id()` must be the exact
/// type which is handled by `handle_raw`. `handle_raw` must
/// interpret any events as the same type.
pub unsafe trait RawEventHandler: Send + Sync + 'static {
    /// Returns the unique ID of this event handler, as allocated by `system_id_for::<T>()`.
    fn id(&self) -> SystemId;

    /// Returns the ID of the event which is handled by this handler.
    fn event_id(&self) -> EventId;

    /// Returns the strategy that should be used to invoke this handler.
    fn strategy(&self) -> HandleStrategy;

    /// Returns the resources read by this event handler.
    fn resource_reads(&self) -> &[ResourceId];
    /// Returns the resources written by this event handler.
    fn resource_writes(&self) -> &[ResourceId];

    /// Handles a slice of events, accessing any needed resources.
    ///
    /// # Safety
    /// * The handler must not access any resources not indicated by `resource_reads()` and `resource_writes()`.
    /// * The given slice __must__ be transmuted to a slice of the event type returned by `event_id`.
    unsafe fn handle_raw_batch(&mut self, events: *const [()], resources: &Resources);
}

// High-level event handlers.

/// An event handler. This type should be used by users, not `RawEventHandler`.
pub trait EventHandler<E: Event>: Send + Sync + 'static {
    /// The resources accessed by this event handler.
    type HandlerData: SystemData;

    /// Handles a single event. Users may implement `handle_batch`
    /// instead which handles multiple events at once.
    fn handle(&self, event: &E, data: &mut Self::HandlerData);

    /// Handles a slice of events. This function may be called instead of `handle`
    /// when multiple events are concerned.
    ///
    /// The default implementation for this function simply calls `handle` on each
    /// event in the slice.
    fn handle_batch(&self, events: &[E], data: &mut Self::HandlerData) {
        events.iter().for_each(|event| self.handle(event, data));
    }

    /// Returns the strategy that should be used to invoke this handler.
    /// The default implementation of this function returns `HandleStrategy::default()`.
    fn strategy(&self) -> HandleStrategy {
        HandleStrategy::default()
    }
}

pub struct CachedEventHandler<H, E>
where
    H: EventHandler<E>,
    E: Event,
{
    inner: H,
    /// Cached system ID.
    id: SystemId,
    /// Cached event ID.
    event_id: EventId,
    /// Cached resource reads.
    resource_reads: Vec<ResourceId>,
    /// Cached resource writes.
    resource_writes: Vec<ResourceId>,
    /// Cached handler data, or `None` if it has not yet been accessed.
    data: Option<H::HandlerData>,
}

impl<H, E> CachedEventHandler<H, E>
where
    H: EventHandler<E>,
    E: Event,
{
    /// Creates a new `CachedEventHandler` caching the given event handler.
    pub fn new(inner: H) -> Self {
        Self {
            id: SYSTEM_ID_MAPPINGS.lock().alloc(),
            event_id: event_id_for::<E>(),
            resource_reads: H::HandlerData::reads(),
            resource_writes: H::HandlerData::writes(),
            data: None,
            inner,
        }
    }
}

unsafe impl<H, E> RawEventHandler for CachedEventHandler<H, E>
where
    H: EventHandler<E>,
    E: Event,
{
    fn id(&self) -> SystemId {
        self.id
    }

    fn event_id(&self) -> EventId {
        self.event_id
    }

    fn strategy(&self) -> HandleStrategy {
        self.inner.strategy()
    }

    fn resource_reads(&self) -> &[ResourceId] {
        &self.resource_reads
    }

    fn resource_writes(&self) -> &[ResourceId] {
        &self.resource_writes
    }

    unsafe fn handle_raw_batch(&mut self, events: *const [()], resources: &Resources) {
        // https://github.com/nvzqz/static-assertions-rs/issues/21
        /*assert_eq_size!(*const [()], *const [H::Event]);
        assert_eq_align!(*const [()], *const [H::Event]);*/

        let events = events as *const [E];
        let events = &*events;

        let data = self
            .data
            .get_or_insert_with(|| H::HandlerData::load_from_resources(resources));

        self.inner.handle_batch(events, data);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn check_event_slice_size_and_align() {
        // temp fix for https://github.com/nvzqz/static-assertions-rs/issues/21
        assert_eq_size!(*const [()], *const [i32]);
        assert_eq_align!(*const [()], *const [i32]);
    }
}
