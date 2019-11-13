use tonks::{Read, Resources, SchedulerBuilder, System, SystemData, Write};

struct Resource1(u32);
struct Resource2(u32);

struct TestSystem1;

impl System for TestSystem1 {
    type SystemData = (Read<Resource1>, Read<Resource2>);

    fn run(&mut self, data: <Self::SystemData as SystemData>::Output) {
        let (r1, r2) = data;

        assert_eq!(r1.0, 3);
        assert_eq!(r2.0, 2);
    }
}

struct TestSystem2;

impl System for TestSystem2 {
    type SystemData = Write<Resource2>;

    fn run(&mut self, r2: <Self::SystemData as SystemData>::Output) {
        r2.0 += 1;
    }
}

struct TestSystem3;

impl System for TestSystem3 {
    type SystemData = Write<Resource1>;

    fn run(&mut self, r1: <Self::SystemData as SystemData>::Output) {
        r1.0 += 2;
    }
}

struct TestSystem4;

impl System for TestSystem4 {
    type SystemData = Read<Resource1>;

    fn run(&mut self, r1: <Self::SystemData as SystemData>::Output) {
        assert_eq!(r1.0, 3);
    }
}

#[test]
fn basic() {
    let mut resources = Resources::new();

    resources.insert(Resource1(1));
    resources.insert(Resource2(1));

    let mut scheduler = SchedulerBuilder::new()
        .with(TestSystem2)
        .with(TestSystem3)
        .with(TestSystem1)
        .with(TestSystem4)
        .build(resources);

    println!("{:?}", scheduler);

    scheduler.execute();
}
