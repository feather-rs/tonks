//! Check various conflicts involving resource access.

use tonks::{EventHandler, EventsBuilder, Read, SchedulerBuilder, System, SystemData, Write};

#[derive(Default)]
struct Resource1(u32);

struct DoubleWrite;

impl System for DoubleWrite {
    // Big UB if not handled correctly!
    type SystemData = (Write<Resource1>, Write<Resource1>);

    fn run(&mut self, _data: <Self::SystemData as SystemData>::Output) {
        panic!("mutable alias reached");
    }
}

impl EventHandler<()> for DoubleWrite {
    type HandlerData = (Write<Resource1>, Write<Resource1>);

    fn handle(&mut self, _event: &(), _data: &mut <Self::HandlerData as SystemData>::Output) {
        panic!("mutable alias reached")
    }
}

struct ReadAndWrite;

impl System for ReadAndWrite {
    // Also UB!
    type SystemData = (Write<Resource1>, Read<Resource1>);

    fn run(&mut self, _data: <Self::SystemData as SystemData>::Output) {
        panic!("mutable alias reached");
    }
}

impl EventHandler<()> for ReadAndWrite {
    type HandlerData = (Write<Resource1>, Read<Resource1>);

    fn handle(&mut self, _event: &(), _data: &mut <Self::HandlerData as SystemData>::Output) {
        panic!("mutable alias reached");
    }
}

#[test]
#[should_panic]
fn double_write() {
    let _ = SchedulerBuilder::new().with(DoubleWrite);
}

#[test]
#[should_panic]
fn double_write_handler() {
    let _ = EventsBuilder::new().with(DoubleWrite);
}

#[test]
#[should_panic]
fn read_and_write() {
    let _ = SchedulerBuilder::new().with(ReadAndWrite);
}

#[test]
#[should_panic]
fn read_and_write_handler() {
    let _ = EventsBuilder::new().with(ReadAndWrite);
}
