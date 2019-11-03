use tonks::{Read, Resources, SchedulerBuilder, System, Write};

struct Resource1(u32);
struct Resource2(u32);

struct TestSystem1;

impl System for TestSystem1 {
    type SystemData = (Read<Resource1>, Read<Resource2>);

    fn run(&mut self, data: &mut Self::SystemData) {
        let (r1, r2) = data;

        assert_eq!(r1.0, 1);
        assert_eq!(r2.0, 2);
    }
}

struct TestSystem2;

impl System for TestSystem2 {
    type SystemData = Write<Resource2>;

    fn run(&mut self, r1: &mut Self::SystemData) {
        r1.0 += 1;
    }
}

#[test]
fn basic() {
    let mut resources = Resources::new();

    resources.insert(Resource1(1));
    resources.insert(Resource2(1));

    let mut scheduler = SchedulerBuilder::new()
        .with(TestSystem2)
        .with(TestSystem1)
        .build(resources);

    scheduler.execute();
}
