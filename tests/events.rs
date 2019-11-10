use tonks::{EventHandler, EventsBuilder, Resources, System, Trigger};

#[test]
fn basic() {
    struct Sys;
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Ev(u32);

    impl System for Sys {
        type SystemData = Trigger<Ev>;

        fn run(&mut self, trigger: &mut Self::SystemData) {
            trigger.trigger(Ev(1));
            trigger.trigger_batched([Ev(2), Ev(3), Ev(5)].iter().copied());
        }
    }

    struct Handler;

    impl EventHandler<Ev> for Handler {
        type HandlerData = ();

        fn handle(&self, _event: &Ev, _data: &mut Self::HandlerData) {
            unreachable!()
        }

        fn handle_batch(&self, events: &[Ev], _data: &mut Self::HandlerData) {
            assert_eq!(events, &[Ev(1), Ev(2), Ev(3), Ev(5)]);
        }
    }

    let mut scheduler = EventsBuilder::new()
        .with(Handler)
        .finish()
        .with(Sys)
        .build(Resources::default());

    for _ in 0..1000 {
        scheduler.execute();
    }
}
