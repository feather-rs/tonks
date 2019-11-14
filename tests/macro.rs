use legion::world::World;
use tonks::{resource_id_for, Resources, SchedulerBuilder};

pub struct Resource1(u32);
pub struct Resource2(u32);

#[test]
fn basic() {
    #[tonks::system]
    fn sys(r1: &Resource1, r2: &mut Resource2) {
        r2.0 += r1.0;
    }

    let mut resources = Resources::new();
    resources.insert(Resource1(10));
    resources.insert(Resource2(5));

    let mut scheduler = SchedulerBuilder::new().with(sys).build(resources);

    scheduler.execute(&mut World::new());

    unsafe {
        assert_eq!(
            scheduler
                .resources()
                .get::<Resource2>(resource_id_for::<Resource2>())
                .0,
            15
        );
    }
}
