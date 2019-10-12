use shred::{Read, Write};
use specs::{World, WorldExt};
use tonks::{Oneshot, Relaxed, SchedulerBuilder, System};

struct TestSystem1;

#[derive(Default)]
struct TestResource(u32);

impl<'a> System<'a> for TestSystem1 {
    type SystemData = Read<'a, TestResource>;
    type Oneshot = Oneshot<Relaxed>;

    fn run(&self, data: Self::SystemData, oneshot: Self::Oneshot) {
        if data.0 == 5 {
            oneshot.schedule(OneshotSystem);
        }
    }
}

struct OneshotSystem;

impl<'a> System<'a> for OneshotSystem {
    type SystemData = Write<'a, TestResource>;
    type Oneshot = Oneshot<Relaxed>;

    fn run(&self, mut data: Self::SystemData, oneshot: Self::Oneshot) {
        data.0 += 1;

        if data.0 == 6 {
            oneshot.schedule(OneshotSystem);
        }
    }
}

#[test]
fn basic_oneshot() {
    let mut scheduler = SchedulerBuilder::new().with(TestSystem1, "", &[]).build();

    let mut world = World::new();
    world.insert(TestResource(5));

    scheduler.execute(&world);
    assert_eq!(world.fetch::<TestResource>().0, 7);

    scheduler.execute(&world);
    assert_eq!(world.fetch::<TestResource>().0, 7);
}
