//! Check various conflicts involving resource access.

use tonks::{Read, SchedulerBuilder, System, Write};

struct Resource1(u32);

struct DoubleWrite;

impl System for DoubleWrite {
    // Big UB if not handled correctly!
    type SystemData = (Write<Resource1>, Write<Resource1>);

    fn run(&mut self, _data: &mut Self::SystemData) {
        panic!("mutable alias reached");
    }
}

struct ReadAndWrite;

impl System for ReadAndWrite {
    // Also UB!
    type SystemData = (Write<Resource1>, Read<Resource1>);

    fn run(&mut self, _data: &mut Self::SystemData) {
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
fn read_and_write() {
    let _ = SchedulerBuilder::new().with(ReadAndWrite);
}
