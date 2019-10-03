//! Trait which abstracts over the running of `System` and `EventHandler`.
use crate::scheduler::SystemToSchedulerMsg;
use crate::{Event, EventBuffer, EventHandler, System};
use crossbeam::Sender;
use specs::World;

pub trait RunNow<'a>: Send {
    fn run_now(&mut self, world: &'a World, sender: Sender<SystemToSchedulerMsg>);
}

impl<'a, T> RunNow<'a> for T
where
    T: System<'a>,
{
    fn run_now(&mut self, world: &'a World, _sender: Sender<SystemToSchedulerMsg>) {
        let data = Self::SystemData::fetch(world);
        self.run(data);
    }
}

impl<'a, T, E> RunNow<'a> for T
where
    T: EventHandler<'a, E>,
    E: Event,
{
    fn run_now(&mut self, world: &'a World, _sender: Sender<SystemToSchedulerMsg>) {
        let mut data = Self::HandlerData::fetch(world);
        let buffer = world.fetch::<EventBuffer<E>>();

        for event in buffer.events() {
            self.handle(&mut data, event);
        }
    }
}
