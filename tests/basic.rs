use shred::Write;
use specs::{World, WorldExt};
use tonks::{SchedulerBuilder, System};

#[derive(Default)]
struct TestResource(usize);

struct TestSystem;

impl<'a> System<'a> for TestSystem {
    type SystemData = Write<'a, TestResource>;

    fn run(&self, mut data: Self::SystemData) {
        data.0 = 1;
    }
}

#[test]
fn basic_one_system() {
    let mut world = World::new();
    world.insert(TestResource(0));

    let mut scheduler = SchedulerBuilder::new().with(TestSystem, "", &[]).build();

    println!("{:?}", scheduler);

    scheduler.execute(&world);
    assert_eq!(world.fetch::<TestResource>().0, 1);
}
