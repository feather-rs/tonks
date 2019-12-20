#![cfg(feature = "system-registry")]

#[macro_use]
extern crate tonks;

#[derive(Resource, Default)]
struct Resource1(u32);

#[system]
fn sys(res: &mut Resource1) {
    res.0 += 1;
}

#[test]
fn basic() {
    use legion::world::World;
    use tonks::Resources;

    let mut resources = Resources::new();
    resources.insert(Resource1(10));

    let mut scheduler = tonks::build_scheduler().build(resources);
    scheduler.execute(&mut World::default());

    assert_eq!(scheduler.resources().get::<Resource1>().0, 11);
}
