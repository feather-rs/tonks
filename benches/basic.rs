#[macro_use]
extern crate criterion;

mod no_dependencies;

criterion_group!(
    no_dependencies,
    no_dependencies::tonks,
    no_dependencies::shred
);
criterion_main!(no_dependencies);
