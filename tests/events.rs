use hashbrown::HashMap;
use legion::world::World;
use std::iter;
use std::sync::atomic::{AtomicUsize, Ordering};
use tonks::{
    resource_id_for, EventHandler, EventsBuilder, Read, Resources, System, SystemData, Trigger,
    Write,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Ev(u32);

#[test]
fn basic() {
    struct Sys;

    impl System for Sys {
        type SystemData = Trigger<Ev>;

        fn run(&mut self, trigger: <Self::SystemData as SystemData>::Output) {
            trigger.trigger(Ev(1));
            trigger.trigger_batched([Ev(2), Ev(3), Ev(5)].iter().copied());
        }
    }

    struct Handler;

    impl EventHandler<Ev> for Handler {
        type HandlerData = ();

        fn handle(&mut self, _event: &Ev, _data: &mut <Self::HandlerData as SystemData>::Output) {
            unreachable!()
        }

        fn handle_batch(
            &mut self,
            events: &[Ev],
            _data: <Self::HandlerData as SystemData>::Output,
        ) {
            assert_eq!(events, &[Ev(1), Ev(2), Ev(3), Ev(5)]);
        }
    }

    let mut scheduler = EventsBuilder::new()
        .with(Handler)
        .finish()
        .with(Sys)
        .build(Resources::default());

    for _ in 0..1000 {
        scheduler.execute(&mut World::new());
    }
}

#[test]
fn zero_sized() {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Ev;

    struct Sys;

    impl System for Sys {
        type SystemData = Trigger<Ev>;

        fn run(&mut self, trigger: <Self::SystemData as SystemData>::Output) {
            trigger.trigger(Ev);
            trigger.trigger_batched(iter::repeat(Ev).take(1023));
        }
    }

    struct Handler;

    impl EventHandler<Ev> for Handler {
        type HandlerData = ();

        fn handle(&mut self, _event: &Ev, _data: &mut <Self::HandlerData as SystemData>::Output) {
            unreachable!()
        }

        fn handle_batch(
            &mut self,
            events: &[Ev],
            _data: <Self::HandlerData as SystemData>::Output,
        ) {
            assert_eq!(events.len(), 1024);
        }
    }

    let mut scheduler = EventsBuilder::new()
        .with(Handler)
        .finish()
        .with(Sys)
        .build(Resources::default());

    for _ in 0..1000 {
        scheduler.execute(&mut World::new());
    }
}

#[test]
fn multi_trigger() {
    struct Sys1;

    impl System for Sys1 {
        type SystemData = Trigger<Ev>;

        fn run(&mut self, trigger: <Self::SystemData as SystemData>::Output) {
            trigger.trigger_batched([Ev(2), Ev(3)].iter().copied());
        }
    }

    struct Sys2;

    impl System for Sys2 {
        type SystemData = Trigger<Ev>;

        fn run(&mut self, trigger: <Self::SystemData as SystemData>::Output) {
            trigger.trigger(Ev(0xFF));
        }
    }

    struct Handler;

    impl EventHandler<Ev> for Handler {
        type HandlerData = Write<HashMap<Ev, usize>>;

        fn handle(&mut self, event: &Ev, count: &mut <Self::HandlerData as SystemData>::Output) {
            *count.entry(*event).or_insert(0) += 1;
        }
    }

    let mut builder = EventsBuilder::new().with(Handler).finish();

    for _ in 0..1024 {
        builder.add(Sys1);
        builder.add(Sys2);
    }

    let mut resources = Resources::new();
    resources.insert(HashMap::<Ev, usize>::default());

    let mut scheduler = builder.build(resources);

    for _ in 0..10 {
        scheduler.execute(&mut World::new());

        let counts = unsafe {
            scheduler
                .resources()
                .get_mut_unchecked::<HashMap<Ev, usize>>(resource_id_for::<HashMap<Ev, usize>>())
        };

        assert_eq!(counts.len(), 3);
        for ev in [Ev(2), Ev(3), Ev(0xFF)].iter() {
            assert_eq!(counts[ev], 1024);
        }

        counts.clear();
    }
}

#[test]
fn multi_handler() {
    struct Sys;

    impl System for Sys {
        type SystemData = Trigger<Ev>;

        fn run(&mut self, trigger: <Self::SystemData as SystemData>::Output) {
            trigger.trigger_batched(iter::repeat(Ev(1)).take(1_000_000));
        }
    }

    struct Handler1;

    impl EventHandler<Ev> for Handler1 {
        type HandlerData = ();

        fn handle(&mut self, event: &Ev, _data: &mut <Self::HandlerData as SystemData>::Output) {
            assert_eq!(event, &Ev(1));
        }
    }

    struct Handler2;

    impl EventHandler<Ev> for Handler2 {
        type HandlerData = ();

        fn handle(&mut self, _event: &Ev, _data: &mut <Self::HandlerData as SystemData>::Output) {
            unreachable!()
        }

        fn handle_batch(
            &mut self,
            events: &[Ev],
            _data: <Self::HandlerData as SystemData>::Output,
        ) {
            assert_eq!(events.len(), 1_000_000);
        }
    }

    let mut scheduler = EventsBuilder::new()
        .with(Handler1)
        .with(Handler2)
        .finish()
        .with(Sys)
        .build(Resources::default());

    for _ in 0..10 {
        scheduler.execute(&mut World::new());
    }
}

#[test]
fn recursive_trigger() {
    struct Sys;

    impl System for Sys {
        type SystemData = Trigger<Ev>;

        fn run(&mut self, trigger: <Self::SystemData as SystemData>::Output) {
            trigger.trigger(Ev(5));
        }
    }

    struct Handler;

    impl EventHandler<Ev> for Handler {
        type HandlerData = (Read<AtomicUsize>, Trigger<Ev>);

        fn handle(
            &mut self,
            event: &Ev,
            (counter, trigger): &mut <Self::HandlerData as SystemData>::Output,
        ) {
            if event.0 > 1 {
                trigger.trigger_batched([Ev(event.0 - 1), Ev(event.0 - 2)].iter().copied());
            } else {
                counter.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    let mut resources = Resources::new();
    resources.insert(AtomicUsize::new(0));

    let mut scheduler = EventsBuilder::new()
        .with(Handler)
        .finish()
        .with(Sys)
        .build(resources);

    for _ in 0..1 {
        scheduler.execute(&mut World::new());

        let count = unsafe {
            scheduler
                .resources()
                .get_unchecked::<AtomicUsize>(resource_id_for::<AtomicUsize>())
                .load(Ordering::Relaxed)
        };

        assert_eq!(count, 8);
    }
}
