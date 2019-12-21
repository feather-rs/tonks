#![cfg(feature = "system-registry")]

#[macro_use]
extern crate tonks;

use tonks::Trigger;

#[derive(Resource, Default)]
struct Resource1(u32);

#[system]
fn sys(res: &mut Resource1, t: &mut Trigger<u32>) {
    res.0 += 1;
    t.trigger(0);
}

#[event_handler]
fn handler(events: &[u32], res: &mut Resource1) {
    res.0 += 1;
    assert_eq!(events[0], 0);
    assert_eq!(events.len(), 1);
}

#[test]
fn basic() {
    use legion::world::World;
    use tonks::Resources;

    let mut resources = Resources::new();
    resources.insert(Resource1(10));

    let mut scheduler = tonks::build_scheduler().build(resources);
    scheduler.execute(&mut World::default());

    assert_eq!(scheduler.resources().get::<Resource1>().0, 12);
}
