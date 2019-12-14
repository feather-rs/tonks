use legion::world::World;
use tonks::{Resources, SchedulerBuilder};

#[macro_use]
extern crate tonks;

#[derive(Default, Resource)]
pub struct Resource1(u32);
#[derive(Default, Resource)]
pub struct Resource2(u32);

#[test]
fn basic() {
    #[tonks::system]
    fn sys(r1: &Resource1, r2: &mut Resource2) {
        r2.0 += r1.0;
    }

    let mut resources = Resources::new();
    resources.insert(Resource1(1));

    let mut scheduler = SchedulerBuilder::new().with(sys).build(resources);

    scheduler.execute(&mut World::new());

    assert_eq!(
        scheduler.resources().get::<Resource2>().0,
        u32::default() + 1
    );
}
