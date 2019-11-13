use criterion::{BenchmarkId, Criterion};
use shred::World;
use specs::{DispatcherBuilder, WorldExt};
use tonks::{Resources, SchedulerBuilder, SystemData};

struct TestSystem;

impl tonks::System for TestSystem {
    type SystemData = ();

    fn run(&mut self, data: &mut <Self::SystemData as SystemData>::Output) {
        unimplemented!()
    }
}

impl<'a> shred::System<'a> for TestSystem {
    type SystemData = ();

    fn run(&mut self, _data: Self::SystemData) {}
}

const SYSTEM_COUNTS: [u32; 6] = [1, 2, 8, 64, 256, 1024];

pub fn tonks(c: &mut Criterion) {
    let mut group = c.benchmark_group("no_dependencies/tonks");

    for count in SYSTEM_COUNTS.iter() {
        let mut builder = SchedulerBuilder::new();

        for _ in 0..*count {
            builder.add(TestSystem);
        }

        let resources = Resources::new();
        let mut scheduler = builder.build(resources);
        let mut world = legion::world::World::new();

        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, _| {
            b.iter(|| {
                scheduler.execute(&mut world);
            })
        });
    }

    group.finish();
}

pub fn shred(c: &mut Criterion) {
    let mut group = c.benchmark_group("no_dependencies/shred");
    let world = World::new();

    for count in SYSTEM_COUNTS.iter() {
        let mut builder = DispatcherBuilder::new();

        for _ in 0..*count {
            builder.add(TestSystem, "", &[]);
        }

        let mut dispatcher = builder.build();

        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, _| {
            b.iter(|| {
                dispatcher.dispatch(&world);
            })
        });
    }

    group.finish();
}
