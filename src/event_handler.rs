use crate::scheduler::{Context, OneshotTuple};
use crate::Event;
use shred::World;
use shred::{ResourceId, SystemData};

pub trait EventHandler<'a>: Send + Sync {
    type Event: Event;
    type HandlerData: SystemData<'a>;
    type Oneshot: OneshotTuple;

    fn handle(&self, event: &Self::Event, data: Self::HandlerData, oneshot: Self::Oneshot);

    fn reads(&self) -> Vec<ResourceId> {
        Self::HandlerData::reads()
    }

    fn writes(&self) -> Vec<ResourceId> {
        Self::HandlerData::writes()
    }
}

pub trait RawEventHandler<'a> {
    /// Handles a raw event.
    ///
    /// # Safety
    /// `event` must point to a valid event corresponding to this
    /// event handler's event type.
    unsafe fn handle_raw(&self, world: &'a World, event: *const (), ctx: Context);
}

impl<'a, H> RawEventHandler<'a> for H
where
    H: EventHandler<'a>,
{
    unsafe fn handle_raw(&self, world: &'a World, event: *const (), ctx: Context) {
        let data = H::HandlerData::fetch(world);

        let event = &*(event as *const H::Event);
        self.handle(&event, data, H::Oneshot::from_ctx(ctx));
    }
}
