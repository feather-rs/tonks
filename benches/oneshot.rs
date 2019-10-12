#[macro_use]
extern crate criterion;

use criterion::{BenchmarkId, Criterion};
use shred::World;
use specs::WorldExt;
use tonks::{Oneshot, Relaxed, SchedulerBuilder, System};

struct TestSystem(u32);

impl<'a> System<'a> for TestSystem {
    type SystemData = ();
    type Oneshot = Oneshot<Relaxed>;

    fn run(&self, _data: Self::SystemData, oneshot: Self::Oneshot) {
        (0..self.0).for_each(|_| oneshot.schedule(OneshotSystem));
    }
}

struct OneshotSystem;

impl<'a> System<'a> for OneshotSystem {
    type SystemData = ();
    type Oneshot = ();

    fn run(&self, _data: Self::SystemData, _oneshot: Self::Oneshot) {
        // Do nothing. We're just measuring scheduler overhead here
    }
}

const COUNTS: [u32; 4] = [1, 8, 32, 128];

fn bench_oneshot(c: &mut Criterion) {
    let mut group = c.benchmark_group("oneshot");
    let world = World::new();

    for count in COUNTS.iter() {
        let mut scheduler = SchedulerBuilder::new()
            .with(TestSystem(*count), "", &[])
            .build();

        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, _| {
            b.iter(|| {
                scheduler.execute(&world);
            });
        });
    }
}

criterion_group!(oneshot, bench_oneshot);
criterion_main!(oneshot);
