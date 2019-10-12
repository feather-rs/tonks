use crate::scheduler::oneshot::OneshotTuple;
use crate::scheduler::Context;
use crate::System;
use shred::{ResourceId, SystemData, World};

pub trait RunNow<'a>: Send + Sync {
    fn run_now(&self, world: &'a World, context: Context);
    fn reads(&self) -> Vec<ResourceId>;
    fn writes(&self) -> Vec<ResourceId>;
}

impl<'a, S> RunNow<'a> for S
where
    S: System<'a>,
{
    fn run_now(&self, world: &'a World, context: Context) {
        let data = S::SystemData::fetch(world);
        let oneshot = S::Oneshot::from_ctx(context);
        self.run(data, oneshot);
    }

    fn reads(&self) -> Vec<ResourceId> {
        S::reads(self)
    }

    fn writes(&self) -> Vec<ResourceId> {
        S::writes(self)
    }
}
