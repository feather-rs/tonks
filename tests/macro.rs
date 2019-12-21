use legion::world::World;
use tonks::{EventsBuilder, Resources, Trigger};

#[macro_use]
extern crate tonks;

#[derive(Default, Resource)]
pub struct Resource1(u32);
#[derive(Default, Resource)]
pub struct Resource2(u32);

pub struct Ev(u32);

#[test]
fn basic() {
    #[system]
    fn sys(r1: &Resource1, r2: &mut Resource2, t: &mut Trigger<Ev>) {
        r2.0 += r1.0;
        t.trigger(Ev(r2.0));
    }

    #[event_handler]
    fn handler(event: &Ev, r2: &Resource2, r1: &mut Resource1) {
        assert_eq!(event.0, r2.0);
        r1.0 = 1_000;
    }

    let mut resources = Resources::new();
    resources.insert(Resource1(1));

    let mut scheduler = EventsBuilder::new()
        .with(handler)
        .finish()
        .with(sys)
        .build(resources);

    scheduler.execute(&mut World::new());

    assert_eq!(
        scheduler.resources().get::<Resource2>().0,
        u32::default() + 1
    );
    assert_eq!(scheduler.resources().get::<Resource1>().0, 1_000);
}
