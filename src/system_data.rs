use crate::{Event, EventBuffer, EventedId};
use shred::{FetchMut, Read, Resource, ResourceId, World, Write};
use smallvec::{smallvec, SmallVec};

pub trait SystemData<'a> {
    fn reads() -> Vec<ResourceId>;
    fn writes() -> Vec<ResourceId>;
    fn event_triggers() -> Vec<EventedId>;
    fn poll_triggered_events(&self) -> SmallVec<[EventedId; 4]>;
    fn fetch(world: &'a World) -> Self;
}

impl<'a, T> SystemData<'a> for Read<'a, T>
where
    T: Resource + Default,
{
    fn reads() -> Vec<ResourceId> {
        vec![ResourceId::new::<T>()]
    }

    fn writes() -> Vec<ResourceId> {
        vec![]
    }

    fn event_triggers() -> Vec<EventedId> {
        vec![]
    }

    fn poll_triggered_events(&self) -> SmallVec<[EventedId; 4]> {
        smallvec![]
    }

    fn fetch(world: &'a World) -> Self {
        <Read<'a, T> as shred::SystemData>::fetch(world)
    }
}

impl<'a, T> SystemData<'a> for Write<'a, T>
where
    T: Resource + Default,
{
    fn reads() -> Vec<ResourceId> {
        vec![]
    }

    fn writes() -> Vec<ResourceId> {
        vec![ResourceId::new::<T>()]
    }

    fn event_triggers() -> Vec<EventedId> {
        vec![]
    }

    fn poll_triggered_events(&self) -> SmallVec<[EventedId; 4]> {
        smallvec![]
    }

    fn fetch(world: &'a World) -> Self {
        <Write<'a, T> as shred::SystemData>::fetch(world)
    }
}

pub struct EventTrigger<'a, E> {
    buffer: FetchMut<'a, EventBuffer<E>>,
    triggered: bool,
}

impl<'a, E> EventTrigger<'a, E>
where
    E: Event,
{
    pub fn trigger(&mut self, event: E) {
        self.triggered = true;
        self.buffer.events.push(event);
    }
}

impl<'a, E> SystemData<'a> for EventTrigger<'a, E>
where
    E: Event,
{
    fn reads() -> Vec<ResourceId> {
        vec![]
    }

    fn writes() -> Vec<ResourceId> {
        vec![]
    }

    fn event_triggers() -> Vec<EventedId> {
        vec![EventedId::new::<E>()]
    }

    fn poll_triggered_events(&self) -> SmallVec<[EventedId; 4]> {
        if !self.buffer.events().is_empty() {
            smallvec![EventedId::new::<E>()]
        } else {
            smallvec![] // No events were triggered
        }
    }

    fn fetch(world: &'a World) -> Self {
        let buffer = world.fetch_mut::<EventBuffer<E>>();
        Self {
            buffer,
            triggered: false,
        }
    }
}
