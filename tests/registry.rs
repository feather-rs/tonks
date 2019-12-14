#[test]
#[cfg(feature = "system-registry")]
fn basic() {
    use legion::world::World;
    use tonks::Resource;
    use tonks::Resources;

    #[derive(Resource, Default)]
    struct Resource1(u32);

    #[tonks::system]
    fn sys(res: &mut Resource1) {
        res.0 += 1;
    }

    let mut resources = Resources::new();
    resources.insert(Resource1(10));

    let mut scheduler = tonks::build_scheduler().build(resources);
    scheduler.execute(&mut World::default());

    assert_eq!(scheduler.resources().get::<Resource1>().0, 11);
}
