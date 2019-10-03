mod scheduler;
mod system_data;

use std::any::{Any, TypeId};
pub use system_data::{EventTrigger, SystemData};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventHandleStratgy {
    Immediate,
    BeforeAccesses,
    Relaxed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EventedId(TypeId);

impl EventedId {
    pub fn new<T>() -> Self
    where
        T: Event,
    {
        EventedId(TypeId::of::<T>())
    }
}

pub trait Event: Any + Send + Sync {
    fn handle_strategy() -> EventHandleStratgy {
        EventHandleStratgy::BeforeAccesses
    }
}

pub trait System<'a>: Send {
    type SystemData: SystemData<'a>;

    fn run(&mut self, data: Self::SystemData);
}

pub trait EventHandler<'a, E>: Send
where
    E: Event,
{
    type HandlerData: SystemData<'a>;

    fn handle(&mut self, data: &mut Self::HandlerData, event: &E);
}

pub struct EventBuffer<E>
where
    E: Event,
{
    events: Vec<E>,
}

impl<E> EventBuffer<E>
where
    E: Event,
{
    pub fn events(&self) -> &[E] {
        &self.events
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }
}
