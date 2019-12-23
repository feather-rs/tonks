#[macro_use]
extern crate tonks;

use legion::entity::Entity;
use legion::query::Read;
use legion::world::World;
use tonks::{PreparedWorld, QueryAccessor, Resources, SchedulerBuilder};

#[derive(Clone, Copy)]
struct Age(u32);

#[derive(Resource)]
struct E(Entity);

#[test]
fn basic() {
    #[system]
    fn sys(accessor: &QueryAccessor<Read<Age>>, world: &mut PreparedWorld, e: &E) {
        let accessor = accessor.find(e.0).unwrap();
        assert_eq!(accessor.get_component::<Age>(world).unwrap().0, 16);
    }

    let mut world = World::new();
    let entity = world.insert((), [(Age(16), 0)].iter().copied())[0];

    let mut resources = Resources::new();
    resources.insert(E(entity));

    let mut scheduler = SchedulerBuilder::new().with(sys).build(resources);

    scheduler.execute(&mut world);
}
