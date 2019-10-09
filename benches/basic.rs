#[macro_use]
extern crate criterion;

mod no_dependencies {
    use criterion::{black_box, BenchmarkId, Criterion};
    use shred::World;
    use specs::{DispatcherBuilder, WorldExt};
    use tonks::SchedulerBuilder;

    struct TestSystem;

    impl<'a> tonks::System<'a> for TestSystem {
        type SystemData = ();

        fn run(&self, data: Self::SystemData) {
            // Do nothing
        }
    }

    impl<'a> shred::System<'a> for TestSystem {
        type SystemData = ();

        fn run(&mut self, data: Self::SystemData) {
            // Do nothing
        }
    }

    const SYSTEM_COUNTS: [u32; 6] = [1, 2, 8, 64, 256, 1024];

    pub fn tonks(c: &mut Criterion) {
        let mut group = c.benchmark_group("no_dependencies/tonks");
        let world = World::new();

        for count in SYSTEM_COUNTS.iter() {
            let mut builder = SchedulerBuilder::new();

            for _ in 0..*count {
                builder.add(TestSystem, "", &[]);
            }

            let mut scheduler = builder.build();

            group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, _| {
                b.iter(|| {
                    scheduler.execute(&world);
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
}

criterion_group!(
    no_dependencies,
    no_dependencies::tonks,
    no_dependencies::shred
);
criterion_main!(no_dependencies);
