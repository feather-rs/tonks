use shred::{Read, Write};
use specs::{World, WorldExt};
use tonks::{SchedulerBuilder, System};

#[derive(Default)]
struct TestResource(usize);

struct TestSystem1;

impl<'a> System<'a> for TestSystem1 {
    type SystemData = Write<'a, TestResource>;

    fn run(&self, mut data: Self::SystemData) {
        data.0 = 1;
    }
}

struct TestSystem2;

impl<'a> System<'a> for TestSystem2 {
    type SystemData = Write<'a, TestResource>;

    fn run(&self, mut data: Self::SystemData) {
        data.0 = 5;
    }
}

struct TestSystem3;

impl<'a> System<'a> for TestSystem3 {
    type SystemData = Read<'a, TestResource>;

    fn run(&self, data: Self::SystemData) {
        assert_eq!(data.0, 5);
    }
}

#[test]
fn basic_three_systems() {
    let mut world = World::new();
    world.insert(TestResource(0));

    let mut scheduler = SchedulerBuilder::new()
        .with(TestSystem1, "1", &[])
        .with(TestSystem2, "2", &["1"])
        .with(TestSystem3, "3", &["2"])
        .build();

    println!("{:?}", scheduler);

    scheduler.execute(&world);
}
