//! Testing of query access.

use hashbrown::HashMap;
use legion::query::Read;
use legion::world::World;
use tonks::{PreparedWorld, Query, Resources, SchedulerBuilder};

#[derive(Debug)]
struct Name(&'static str);
#[derive(Debug, Clone, Copy)]
struct Age(u32);

#[test]
fn basic() {
    let mut world = World::new();

    world.insert(
        (),
        vec![
            (Name("Jar Jar Binks"), Age(2)),
            (Name("Bill Gates"), Age(64)),
            (Name("Donald Trump"), Age(3)),
        ],
    );
    world.insert((), vec![(Name("Undefined"), 1.0 / 0.0)]);

    #[tonks::system]
    fn sys(query: &mut Query<(Read<Name>, Read<Age>)>, world: &mut PreparedWorld) {
        let mut ages = HashMap::new();

        for (name, age) in query.iter(world) {
            ages.insert(name.0, age.0);
        }

        assert_eq!(ages["Jar Jar Binks"], 2);
        assert_eq!(ages["Bill Gates"], 64);
        assert_eq!(ages["Donald Trump"], 3);
        assert!(!ages.contains_key("Undefined"));
    }

    let mut scheduler = SchedulerBuilder::new()
        .with(sys)
        .build(Resources::default());

    for _ in 0..2 {
        scheduler.execute(&mut world);
    }
}
