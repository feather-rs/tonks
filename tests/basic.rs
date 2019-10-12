use shred::{Read, Write};
use specs::{World, WorldExt};
use tonks::{SchedulerBuilder, System};

#[derive(Default)]
struct TestResource(usize);

struct TestSystem1;

impl<'a> System<'a> for TestSystem1 {
    type SystemData = Write<'a, TestResource>;
    type Oneshot = ();

    fn run(&self, mut data: Self::SystemData, _oneshot: Self::Oneshot) {
        data.0 = 1;
    }
}

struct TestSystem2;

impl<'a> System<'a> for TestSystem2 {
    type SystemData = Write<'a, TestResource>;
    type Oneshot = ();

    fn run(&self, mut data: Self::SystemData, _oneshot: Self::Oneshot) {
        data.0 = 5;
    }
}

struct TestSystem3;

impl<'a> System<'a> for TestSystem3 {
    type SystemData = Read<'a, TestResource>;
    type Oneshot = ();

    fn run(&self, data: Self::SystemData, _oneshot: Self::Oneshot) {
        assert_eq!(data.0, 5);
    }
}

struct TestSystem4;

impl<'a> System<'a> for TestSystem4 {
    type SystemData = Read<'a, TestResource>;
    type Oneshot = ();

    fn run(&self, data: Self::SystemData, _oneshot: Self::Oneshot) {
        assert_eq!(data.0, 5);
    }
}

#[test]
fn basic_four_systems() {
    let mut world = World::new();
    world.insert(TestResource(0));

    let mut scheduler = SchedulerBuilder::new()
        .with(TestSystem1, "1", &[])
        .with(TestSystem2, "2", &["1"])
        .with(TestSystem3, "3", &["2"])
        .with(TestSystem4, "4", &[])
        .build();

    println!("{:?}", scheduler);

    scheduler.execute(&world);
}

#[test]
fn concurrent_reads() {
    let mut world = World::new();
    world.insert(TestResource(5));

    let mut builder = SchedulerBuilder::new();

    for _ in 0..128 {
        builder.add(TestSystem3, "", &[]);
    }

    let mut scheduler = builder.build();

    println!("{:?}", scheduler);

    scheduler.execute(&world);
}

#[test]
#[should_panic]
fn duplicate_system_names() {
    let _ = SchedulerBuilder::new()
        .with(TestSystem1, "1", &[])
        .with(TestSystem2, "2", &["1"])
        .with(TestSystem3, "2", &["2"])
        .build();
}

#[test]
#[should_panic]
fn unknown_runs_after() {
    let _ = SchedulerBuilder::new()
        .with(TestSystem1, "1", &[])
        .with(TestSystem2, "2", &["1"])
        .with(TestSystem3, "3", &["3"])
        .build();
}
