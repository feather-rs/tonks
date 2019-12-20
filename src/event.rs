use crate::mappings::Mappings;
use crate::scheduler::TaskMessage;
use crate::system::{SystemCtx, SystemDataOutput, SYSTEM_ID_MAPPINGS};
use crate::{MacroData, ResourceId, Resources, SystemData, SystemId};
use lazy_static::lazy_static;
use legion::storage::ComponentTypeId;
use legion::world::World;
use parking_lot::Mutex;
use std::alloc::Layout;
use std::any::TypeId;
use std::ptr;

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
    EndOfSystem,
    /// The handle will be scheduled for running at the end of tick.
    ///
    /// This is the default strategy.
    EndOfTick,
}

impl Default for HandleStrategy {
    fn default() -> Self {
        HandleStrategy::EndOfTick
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

    fn init(&mut self, resources: &mut Resources, ctx: SystemCtx, world: &World);

    /// Handles a slice of events, accessing any needed resources.
    ///
    /// # Safety
    /// * The handler must not access any resources not indicated by `resource_reads()` and `resource_writes()`.
    /// * The given slice __must__ be transmuted to a slice of the event type returned by `event_id`.
    unsafe fn handle_raw_batch(
        &mut self,
        events: *const (),
        events_len: usize,
        resources: &Resources,
        ctx: SystemCtx,
        world: &World,
    );
}

// High-level event handlers.

/// An event handler. This type should be used by users, not `RawEventHandler`.
pub trait EventHandler<E: Event>: Send + Sync + 'static {
    /// The resources accessed by this event handler.
    type HandlerData: for<'a> SystemData<'a>;

    /// Handles a single event. Users may implement `handle_batch`
    /// instead which handles multiple events at once.
    fn handle(&mut self, event: &E, data: &mut <Self::HandlerData as SystemData>::Output);

    /// Handles a slice of events. This function may be called instead of `handle`
    /// when multiple events are concerned.
    ///
    /// The default implementation for this function simply calls `handle` on each
    /// event in the slice.
    fn handle_batch(&mut self, events: &[E], mut data: <Self::HandlerData as SystemData>::Output) {
        events
            .iter()
            .for_each(|event| self.handle(event, &mut data));
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
    /// Cached component reads.
    component_reads: Vec<ComponentTypeId>,
    /// Cached component writes.
    component_writes: Vec<ComponentTypeId>,
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
            resource_reads: H::HandlerData::resource_reads(),
            resource_writes: H::HandlerData::resource_writes(),
            component_reads: H::HandlerData::component_reads(),
            component_writes: H::HandlerData::component_writes(),
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

    fn init(&mut self, resources: &mut Resources, ctx: SystemCtx, world: &World) {
        let mut data = unsafe { H::HandlerData::load_from_resources(resources, ctx, world) };
        data.init(resources, &self.component_reads, &self.component_writes);
        self.data = Some(data);
    }

    unsafe fn handle_raw_batch(
        &mut self,
        events: *const (),
        events_len: usize,
        _resources: &Resources,
        _ctx: SystemCtx,
        _world: &World,
    ) {
        // https://github.com/nvzqz/static-assertions-rs/issues/21
        /*assert_eq_size!(*const [()], *const [H::Event]);
        assert_eq_align!(*const [()], *const [H::Event]);*/

        let events = std::slice::from_raw_parts(events as *const E, events_len);

        let data = self.data.as_mut().unwrap();

        self.inner.handle_batch(events, data.before_execution());

        data.after_execution();
    }
}

/// System data which allows you to trigger events of a given type.
pub struct Trigger<E>
where
    E: Event,
{
    ctx: SystemCtx,
    queued: Vec<E>,
    id: EventId,
}

impl<'a, E> SystemData<'a> for Trigger<E>
where
    E: Event,
{
    type Output = &'a mut Self;

    unsafe fn load_from_resources(
        _resources: &mut Resources,
        ctx: SystemCtx,
        _world: &World,
    ) -> Self {
        Self {
            ctx,
            queued: vec![],
            id: event_id_for::<E>(),
        }
    }

    fn resource_reads() -> Vec<ResourceId> {
        vec![]
    }

    fn resource_writes() -> Vec<ResourceId> {
        vec![]
    }

    fn component_reads() -> Vec<ComponentTypeId> {
        vec![]
    }

    fn component_writes() -> Vec<ComponentTypeId> {
        vec![]
    }

    fn before_execution(&'a mut self) -> Self::Output {
        self
    }

    fn after_execution(&mut self) {
        // TODO: end-of-system handlers

        // Move events to bump-allocated slice and send to scheduler.
        let len = self.queued.len();

        if len == 0 {
            return; // Nothing to do
        }

        let ptr: *mut E = self
            .ctx
            .bump
            .get_or_default()
            .alloc_layout(Layout::for_value(self.queued.as_slice()))
            .cast::<E>()
            .as_ptr();

        self.queued
            .drain(..)
            .enumerate()
            .for_each(|(index, event)| unsafe {
                ptr::write(ptr.offset(index as isize), event);
            });

        self.ctx
            .sender
            .send(TaskMessage::TriggerEvents {
                id: self.id,
                ptr: ptr as *const (),
                len,
            })
            .unwrap();
    }
}

impl<E> Trigger<E>
where
    E: Send + Sync + 'static,
{
    pub fn trigger(&mut self, event: E) {
        self.queued.push(event);
    }

    pub fn trigger_batched(&mut self, events: impl IntoIterator<Item = E>) {
        self.queued.extend(events);
    }
}

impl<'a, E> SystemDataOutput<'a> for &'a mut Trigger<E>
where
    E: Send + Sync + 'static,
{
    type SystemData = Trigger<E>;
}

impl<E> MacroData for &'static mut Trigger<E>
where
    E: Event,
{
    type SystemData = Trigger<E>;
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
