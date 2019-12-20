#[macro_use]
extern crate tonks;

use legion::entity::Entity;
use legion::query::Read;
use legion::world::World;
use tonks::{PreparedWorld, Query, Resources, SchedulerBuilder};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Age(u32);
#[derive(Resource)]
struct E(Entity);

#[test]
fn basic() {
    #[system]
    fn sys(_query: &mut Query<Read<Age>>, world: &mut PreparedWorld, e: &E) {
        assert_eq!(*world.get_component::<Age>(e.0).unwrap(), Age(10));
    }

    let mut world = World::new();
    let entity = world.insert((), [(Age(10), 2)].iter().copied())[0];

    let mut resources = Resources::new();
    resources.insert(E(entity));

    let mut scheduler = SchedulerBuilder::new().with(sys).build(resources);

    scheduler.execute(&mut world);
}
